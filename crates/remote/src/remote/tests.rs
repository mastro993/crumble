use super::{Listed, Remote, data_ids};
use crate::{Job, KeyCheck, KeyFormat, Provider};
use regex::Regex;
use serde_json::json;

fn remote(provider: Provider) -> &'static Remote {
    match provider.remote() {
        Some(remote) => remote,
        None => panic!("{provider:?} isn't remote"),
    }
}

fn key(provider: Provider) -> &'static KeyFormat {
    match remote(provider).api_key() {
        Some(format) => format,
        None => panic!("{provider:?} takes no key"),
    }
}

/// What each remote Provider declares: subscription only, chats, transcribes, streams.
#[test]
fn each_provider_declares_what_it_does() {
    let declared = [
        (Provider::Anthropic, [false, true, false, true]),
        (Provider::OpenAi, [false, true, true, true]),
        (Provider::Gemini, [false, true, true, true]),
        (Provider::ElevenLabs, [false, false, true, false]),
        (Provider::OpenRouter, [false, true, true, true]),
        (Provider::DeepSeek, [false, true, false, true]),
        (Provider::Xai, [false, true, true, true]),
        (Provider::OpenAiCompatible, [false, true, true, true]),
    ];
    assert_eq!(declared.len(), Provider::REMOTE.len());
    for (provider, flags) in declared {
        let remote = remote(provider);
        assert_eq!(
            [
                remote.subscription_only(),
                remote.does(Job::Intelligence),
                remote.does(Job::Transcription),
                remote.streams(),
            ],
            flags,
            "{provider:?}"
        );
    }
}

#[test]
fn every_pattern_compiles_and_every_suggestion_is_offered() {
    for provider in Provider::REMOTE {
        let remote = remote(provider);
        if let Some(format) = remote.api_key() {
            let patterns = format.pattern.into_iter();
            for pattern in patterns.chain(format.refused.iter().map(|(pattern, _)| *pattern)) {
                assert!(Regex::new(pattern).is_ok(), "{provider:?}: {pattern}");
            }
        }
        for job in [Job::Transcription, Job::Summary, Job::Intelligence] {
            if let Some(model) = remote.default_model(job) {
                assert!(
                    remote
                        .models(job, None)
                        .iter()
                        .any(|offered| offered == model),
                    "{provider:?} {job:?} {model}"
                );
            }
            assert_eq!(
                remote.models(job, None).is_empty(),
                !remote.does(job) || remote.default_model(job).is_none(),
                "{provider:?} {job:?}"
            );
        }
    }
}

#[test]
fn real_key_shapes_pass_and_other_providers_keys_do_not() {
    let hex = |n| "0123456789abcdef".repeat(8)[..n].to_owned();
    let anthropic = format!("sk-ant-api03-{}AA", "Ab_-9".repeat(19)[..93].to_owned());
    let openai = format!("sk-proj-{}T3BlbkFJ{}", "a".repeat(74), "b".repeat(74));
    let gemini = format!("AIza{}", "x".repeat(35));
    let elevenlabs = format!("sk_{}", hex(48));
    let openrouter = format!("sk-or-v1-{}", hex(64));
    let deepseek = format!("sk-{}", hex(32));
    let xai = format!("xai-{}", "Z9".repeat(40));
    let keys = [
        (Provider::Anthropic, anthropic.as_str()),
        (Provider::OpenAi, openai.as_str()),
        (Provider::Gemini, gemini.as_str()),
        (Provider::ElevenLabs, elevenlabs.as_str()),
        (Provider::OpenRouter, openrouter.as_str()),
        (Provider::DeepSeek, deepseek.as_str()),
        (Provider::Xai, xai.as_str()),
    ];
    for provider in Provider::REMOTE {
        let patterned = (remote(provider).api_key()).is_some_and(|format| format.pattern.is_some());
        assert_eq!(
            keys.iter().any(|(listed, _)| *listed == provider),
            patterned,
            "{provider:?} needs a real-shaped key in `keys`"
        );
    }
    for (provider, own) in keys {
        assert_eq!(key(provider).check(own), KeyCheck::Valid, "{provider:?}");
        for (other, other_key) in keys {
            if other != provider {
                assert!(
                    matches!(key(provider).check(other_key), KeyCheck::Invalid(_)),
                    "{provider:?} took {other:?}'s key"
                );
            }
        }
    }
    // Anthropic's personal and service account keys, and a kind it may add later.
    for kind in [
        "sk-ant-usr01-",
        "sk-ant-svc01-",
        "sk-ant-usr-",
        "sk-ant-new01-",
    ] {
        let identity = format!("{kind}{}AA", "Ab_-9".repeat(19)[..93].to_owned());
        assert_eq!(
            key(Provider::Anthropic).check(&identity),
            KeyCheck::Valid,
            "{kind}"
        );
    }
    for refused in ["sk-ant-admin01-", "sk-ant-oat01-", "sk-ant-ort01-"] {
        let refused = format!("{refused}{}AA", "a".repeat(93));
        assert!(
            matches!(
                key(Provider::Anthropic).check(&refused),
                KeyCheck::Invalid(_)
            ),
            "{refused}"
        );
    }
    // Keys of another Provider that start the same way are named, not called incomplete.
    assert_eq!(
        key(Provider::DeepSeek).check(&openai),
        KeyCheck::Invalid("This is an OpenAI key.".to_owned())
    );
    assert_eq!(
        key(Provider::DeepSeek).check(&anthropic),
        KeyCheck::Invalid("This is an Anthropic key.".to_owned())
    );
    assert_eq!(
        key(Provider::OpenAi).check(&deepseek),
        KeyCheck::Invalid("This is a DeepSeek key.".to_owned())
    );
    let admin = format!("sk-admin-{}", "a".repeat(60));
    assert!(matches!(
        key(Provider::OpenAi).check(&admin),
        KeyCheck::Invalid(_)
    ));
    let server = key(Provider::OpenAiCompatible);
    assert!(!server.required());
    assert_eq!(server.check("anything at all"), KeyCheck::Valid);
}

#[test]
fn a_picked_model_replaces_the_default_for_the_jobs_it_does() {
    let openai = remote(Provider::OpenAi);
    assert_eq!(openai.model(Job::Summary, None), Some("gpt-6.1-sol"));
    assert_eq!(openai.model(Job::Summary, Some(" ")), Some("gpt-6.1-sol"));
    assert_eq!(openai.model(Job::Summary, Some(" gpt-9 ")), Some("gpt-9"));
    assert_eq!(openai.model(Job::Transcription, None), Some("whisper-1"));
    let anthropic = remote(Provider::Anthropic);
    assert_eq!(anthropic.model(Job::Transcription, Some("whisper-1")), None);
    let server = remote(Provider::OpenAiCompatible);
    assert_eq!(server.model(Job::Summary, None), None);
    assert_eq!(
        server.model(Job::Summary, Some("llama3.2")),
        Some("llama3.2")
    );
}

#[test]
fn a_provider_is_set_up_with_its_key_and_an_address() {
    let openai = remote(Provider::OpenAi);
    assert!(!openai.is_set_up(false, ""));
    assert!(openai.is_set_up(true, ""));
    assert!(!openai.is_set_up(true, "api.example.com/v1"));
    let server = remote(Provider::OpenAiCompatible);
    assert!(!server.is_set_up(true, ""));
    assert!(server.is_set_up(false, "http://localhost:11434/v1"));
}

#[test]
fn a_saved_address_replaces_the_usual_one() {
    let openai = remote(Provider::OpenAi);
    assert_eq!(openai.base_url(""), "https://api.openai.com/v1");
    assert_eq!(openai.base_url(" "), "https://api.openai.com/v1");
    assert_eq!(
        openai.base_url(" http://localhost:4000/v1 "),
        "http://localhost:4000/v1"
    );
    let server = remote(Provider::OpenAiCompatible);
    assert_eq!(server.base_url(""), "");
}

#[test]
fn requests_carry_the_key_in_its_header_and_the_provider_s_own() {
    let anthropic = remote(Provider::Anthropic);
    assert_eq!(
        anthropic.headers(Some("k1")),
        [
            ("x-api-key", "k1".to_owned()),
            ("anthropic-version", "2023-06-01".to_owned()),
        ]
    );
    let server = remote(Provider::OpenAiCompatible);
    assert_eq!(server.headers(None), []);
    assert_eq!(server.headers(Some(" ")), []);
    assert_eq!(
        server.headers(Some("k1")),
        [("authorization", "Bearer k1".to_owned())]
    );
}

#[test]
fn offered_models_put_suggestions_first_then_the_listed_ones_once() {
    let openai = remote(Provider::OpenAi);
    let listed = Listed {
        chat: vec!["gpt-6-luna".to_owned(), "gpt-9".to_owned()],
        speech: vec![],
    };
    let offered = openai.models(Job::Summary, Some(&listed));
    assert_eq!(offered.first().map(String::as_str), Some("gpt-6.1-sol"));
    assert_eq!(offered.iter().filter(|m| *m == "gpt-6-luna").count(), 1);
    assert_eq!(offered.last().map(String::as_str), Some("gpt-9"));
    let deepseek = remote(Provider::DeepSeek);
    assert!(
        deepseek
            .models(Job::Transcription, Some(&listed))
            .is_empty()
    );
}

#[test]
fn openai_style_lists_give_their_ids_sorted_once() {
    let answer = json!({"data": [{"id": "b"}, {"id": "a"}, {"id": "b"}, {"x": 1}, {"id": "c"}]});
    assert_eq!(data_ids(&answer, |id| id != "c"), ["a", "b"]);
    assert!(data_ids(&json!({}), |_| true).is_empty());
}
