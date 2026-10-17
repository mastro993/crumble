// The C ABI that `crumble-summarize` calls (see src/ffi.rs and ADR 0007). It drives Apple's
// on-device Foundation Models and nothing else: each function blocks its caller, a Rust
// background thread, until the async work it starts on Swift's own threads is done.
//
// Strings go out as `strdup` copies the caller frees with `crumble_summarize_free`. A function
// that can fail returns 0 and its result in `out`, or 1 and an error message in `out`.

import Foundation
import FoundationModels

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

/// Writes `result` to `out`: 0 and the value, or 1 and the error's message.
private func answer(_ result: Result<String, Error>, _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> Int32 {
  switch result {
  case .success(let value):
    out.pointee = strdup(value)
    return 0
  case .failure(let error):
    out.pointee = strdup(message(error))
    return 1
  }
}

/// What to tell the user when the model fails.
private func message(_ error: Error) -> String {
  guard let error = error as? LanguageModelSession.GenerationError else {
    return error.localizedDescription
  }
  switch error {
  case .exceededContextWindowSize:
    return "The Transcript was too long for Apple's on-device model."
  case .guardrailViolation, .refusal:
    return "Apple's on-device model declined to summarize this Transcript."
  case .unsupportedLanguageOrLocale:
    return "Apple's on-device model can't summarize this Transcript's language."
  case .assetsUnavailable:
    return "Apple's on-device model isn't ready. Try again later."
  case .rateLimited, .concurrentRequests:
    return "Apple's on-device model is busy. Try again in a moment."
  default:
    return error.localizedDescription
  }
}

/// The model Crumble summarizes with. Summarizing rewrites what was said, so it takes the
/// guardrails meant for transforming content the user supplies.
private func model() -> SystemLanguageModel {
  SystemLanguageModel(useCase: .general, guardrails: .permissiveContentTransformations)
}

/// 0: available; 1: this Mac can't run it; 2: Apple Intelligence is off; 3: the model is still
/// downloading; 4: unavailable for another reason.
@_cdecl("crumble_summarize_availability")
public func availability() -> Int32 {
  switch SystemLanguageModel.default.availability {
  case .available: return 0
  case .unavailable(.deviceNotEligible): return 1
  case .unavailable(.appleIntelligenceNotEnabled): return 2
  case .unavailable(.modelNotReady): return 3
  default: return 4
  }
}

/// How many tokens the model reads at once: its prompt, instructions and response together.
@_cdecl("crumble_summarize_context_size")
public func contextSize() -> Int32 {
  Int32(clamping: SystemLanguageModel.default.contextSize)
}

/// The token count of each of `texts`, which are separated by U+001E. `out` gets the counts,
/// one per line in the same order, or the error. Before macOS 26.4 there is no tokenizer to
/// ask, so a count is estimated at one token per three UTF-8 bytes, which overcounts.
@_cdecl("crumble_summarize_token_counts")
public func tokenCounts(
  _ texts: UnsafePointer<CChar>,
  _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
  let texts = String(cString: texts).split(separator: "\u{1e}", omittingEmptySubsequences: false).map(String.init)
  let result: Result<String, Error> = blocking {
    do {
      var counts: [String] = []
      for text in texts {
        if #available(macOS 26.4, *) {
          counts.append(String(try await model().tokenCount(for: text)))
        } else {
          counts.append(String(text.utf8.count / 3 + 1))
        }
      }
      return .success(counts.joined(separator: "\n"))
    } catch {
      return .failure(error)
    }
  }
  return answer(result, out)
}

/// Answers `prompt` in a new session following `instructions`, in at most `maxTokens` tokens.
/// `out` gets the answer, or the error.
@_cdecl("crumble_summarize_respond")
public func respond(
  _ instructions: UnsafePointer<CChar>,
  _ prompt: UnsafePointer<CChar>,
  _ maxTokens: Int32,
  _ out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Int32 {
  let instructions = String(cString: instructions)
  let prompt = String(cString: prompt)
  let result: Result<String, Error> = blocking {
    do {
      let session = LanguageModelSession(model: model(), instructions: instructions)
      let options = GenerationOptions(maximumResponseTokens: Int(maxTokens))
      return .success(try await session.respond(to: prompt, options: options).content)
    } catch {
      return .failure(error)
    }
  }
  return answer(result, out)
}

/// Frees a string this bridge returned.
@_cdecl("crumble_summarize_free")
public func free(_ string: UnsafeMutablePointer<CChar>?) {
  Darwin.free(string)
}
