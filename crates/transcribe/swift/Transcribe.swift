// The C ABI that `crumble-transcribe` calls (see src/ffi.rs and ADR 0006). It drives Apple's
// SpeechAnalyzer and nothing else: each function blocks its caller, a Rust background thread,
// until the async work it starts on Swift's own threads is done.
//
// Strings go out as `strdup` copies the caller frees with `crumble_transcribe_free`. A function
// that can fail returns 0 and its result in `out`, or 1 and an error message in `out`.

import AVFAudio
import Foundation
import Speech

/// Receives the caller's context and how far the work is, 0 to 1.
public typealias ProgressCallback = @convention(c) (UnsafeMutableRawPointer?, Float) -> Void

/// The value an async task hands back to the thread waiting for it.
private final class Handoff<T>: @unchecked Sendable {
  var value: T?
}

/// Runs `operation` on Swift's concurrency threads and blocks the calling thread until it ends.
/// The caller is never one of those threads, so this cannot starve them.
private func blocking<T>(_ operation: @escaping () async -> T) -> T {
  let done = DispatchSemaphore(value: 0)
  let handoff = Handoff<T>()
  Task.detached {
    handoff.value = await operation()
    done.signal()
  }
  done.wait()
  return handoff.value!
}

private struct BridgeError: LocalizedError {
  let errorDescription: String?
  init(_ message: String) { errorDescription = message }
}

/// Writes `result` to `out`: 0 and the value, or 1 and the error's message.
private func answer(_ result: Result<String, Error>, _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> Int32 {
  switch result {
  case .success(let value):
    out.pointee = strdup(value)
    return 0
  case .failure(let error):
    out.pointee = strdup(error.localizedDescription)
    return 1
  }
}

/// The supported locale `identifier` names, or nil. `supportedLocale(equivalentTo:)` can echo
/// back a locale Apple does not transcribe, so its answer is checked against the list.
private func supportedLocale(_ identifier: String) async -> Locale? {
  guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: identifier)) else {
    return nil
  }
  let tag = locale.identifier(.bcp47)
  let supported = await SpeechTranscriber.supportedLocales
  return supported.contains { $0.identifier(.bcp47) == tag } ? locale : nil
}

/// The transcriber Crumble uses: final results only, each with its time range.
private func transcriber(_ locale: Locale) -> SpeechTranscriber {
  SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [], attributeOptions: [.audioTimeRange])
}

/// The current locale's identifier, such as `en_US`.
@_cdecl("crumble_transcribe_current_locale")
public func currentLocale() -> UnsafeMutablePointer<CChar>? {
  strdup(Locale.current.identifier)
}

/// `locale`'s name in the current locale, such as `English (United States)`.
@_cdecl("crumble_transcribe_locale_name")
public func localeName(_ locale: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
  let identifier = String(cString: locale)
  return strdup(Locale.current.localizedString(forIdentifier: identifier) ?? identifier)
}

/// 0: `locale` cannot be transcribed on this Mac; 1: its model can be downloaded; 2: it is
/// downloading; 3: it is installed.
@_cdecl("crumble_transcribe_model_status")
public func modelStatus(_ locale: UnsafePointer<CChar>) -> Int32 {
  let identifier = String(cString: locale)
  return blocking {
    guard SpeechTranscriber.isAvailable, let locale = await supportedLocale(identifier) else {
      return 0
    }
    switch await AssetInventory.status(forModules: [transcriber(locale)]) {
    case .unsupported: return 0
    case .supported: return 1
    case .downloading: return 2
    case .installed: return 3
    @unknown default: return 0
    }
  }
}

/// Downloads and installs `locale`'s model, reporting its progress. `out` gets an empty string,
/// or the error.
@_cdecl("crumble_transcribe_install_model")
public func installModel(
  _ locale: UnsafePointer<CChar>,
  _ progress: ProgressCallback,
  _ context: UnsafeMutableRawPointer?,
  _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
  let identifier = String(cString: locale)
  let result: Result<String, Error> = blocking {
    do {
      guard let locale = await supportedLocale(identifier) else {
        throw BridgeError("Transcription isn't available for \(identifier) on this Mac.")
      }
      let module = transcriber(locale)
      if let request = try await AssetInventory.assetInstallationRequest(supporting: [module]) {
        // Progress is polled, as KVO would need an NSObject observer. The watcher is awaited
        // before anything else reports, so the callback never runs twice at once.
        let watcher = Task.detached {
          while !Task.isCancelled {
            progress(context, Float(request.progress.fractionCompleted))
            try? await Task.sleep(for: .milliseconds(250))
          }
        }
        let downloaded = await Task { try await request.downloadAndInstall() }.result
        watcher.cancel()
        await watcher.value
        try downloaded.get()
      }
      // A download can return while the system still installs it; give that a few minutes.
      let deadline = ContinuousClock.now + .seconds(300)
      while true {
        switch await AssetInventory.status(forModules: [module]) {
        case .installed:
          progress(context, 1)
          return .success("")
        case .downloading where ContinuousClock.now < deadline,
          .supported where ContinuousClock.now < deadline:
          try await Task.sleep(for: .milliseconds(500))
        case .unsupported:
          throw BridgeError("Transcription isn't available for \(identifier) on this Mac.")
        default:
          throw BridgeError("The speech model for \(identifier) did not install.")
        }
      }
    } catch {
      return .failure(error)
    }
  }
  return answer(result, out)
}

/// Transcribes the audio file at `path` in `locale`, whose model must be installed. `out` gets
/// one line per stretch of speech, `start_ms\tend_ms\ttext`, in time order, or the error.
@_cdecl("crumble_transcribe_file")
public func transcribeFile(
  _ path: UnsafePointer<CChar>,
  _ locale: UnsafePointer<CChar>,
  _ progress: ProgressCallback,
  _ context: UnsafeMutableRawPointer?,
  _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
  let path = String(cString: path)
  let identifier = String(cString: locale)
  let result: Result<String, Error> = blocking {
    do {
      guard let locale = await supportedLocale(identifier) else {
        throw BridgeError("Transcription isn't available for \(identifier) on this Mac.")
      }
      let file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
      let seconds = Double(file.length) / file.processingFormat.sampleRate
      let module = transcriber(locale)
      let analyzer = SpeechAnalyzer(modules: [module])
      let lines = Task {
        var lines: [String] = []
        for try await result in module.results where result.isFinal {
          let text = String(result.text.characters)
            .replacingOccurrences(of: "\t", with: " ")
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespaces)
          let end = result.range.end.seconds
          if seconds > 0 {
            progress(context, Float(min(end / seconds, 1)))
          }
          guard !text.isEmpty else { continue }
          let start = Int((result.range.start.seconds * 1000).rounded())
          lines.append("\(start)\t\(Int((end * 1000).rounded()))\t\(text)")
        }
        return lines
      }
      do {
        if let last = try await analyzer.analyzeSequence(from: file) {
          try await analyzer.finalizeAndFinish(through: last)
        } else {
          await analyzer.cancelAndFinishNow()
        }
      } catch {
        // The collector may still report progress: it ends before the caller's context goes.
        await analyzer.cancelAndFinishNow()
        lines.cancel()
        _ = await lines.result
        throw error
      }
      let text = try await lines.value.joined(separator: "\n")
      progress(context, 1)
      return .success(text)
    } catch {
      return .failure(error)
    }
  }
  return answer(result, out)
}

/// Frees a string this bridge returned.
@_cdecl("crumble_transcribe_free")
public func free(_ string: UnsafeMutablePointer<CChar>?) {
  Darwin.free(string)
}
