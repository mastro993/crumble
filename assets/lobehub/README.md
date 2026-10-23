# Provider logos

These are the logos Settings shows next to each remote Provider, in the Providers list, and next to Apple's and each remote Provider's row in the Transcription and Intelligence Provider selects. They come from [LobeHub Icons](https://github.com/lobehub/lobe-icons), MIT licensed (see [LICENSE.md](LICENSE.md)). Each logo is a trademark of its owner and is shown only to identify that Provider.

Each file is named after its Provider's id, `Provider::id()` in `crumble-remote`. Every file was downloaded from npm `@lobehub/icons-static-svg` version 1.97.0:

```
https://unpkg.com/@lobehub/icons-static-svg@1.97.0/icons/<slug>.svg
```

| File | Slug | Kind | sha256 of the download |
|---|---|---|---|
| `apple.svg` | `apple` | mono | `100637681febc38512bf8d43f7d5f79f2aa8caead15b256cd28b378b053fca69` |
| `anthropic.svg` | `anthropic` | mono | `e833fdfa7e7187a86a05b870925067556a506dcd4239089aed73e1e58c9366a3` |
| `openai.svg` | `openai` | mono | `a595df6b423920c67a7f8f73c063e4bfb72d415948097b6cac063a2366bb5186` |
| `gemini.svg` | `gemini-color` | color | `8ab0a9bafec11f7e69bcb9fc4ffd8f1bc927d1ddcbbb6ff36dee5ae8b5a9d602` |
| `elevenlabs.svg` | `elevenlabs` | mono | `152a4a89fd5228924b6ccc91a72675966df35e3b69d1cf93a67b6866cf846584` |
| `openrouter.svg` | `openrouter` | mono | `ae671cc83de9bd45db97a47ed5e38a4e1bd9f93a12be495ba4b4f8b284026bca` |
| `deepseek.svg` | `deepseek-color` | color | `deba5f98a5c1796e20fcac3149bcd7eb8a32f0bdd04d048819400b1f28bd1439` |
| `xai.svg` | `xai` | mono | `89eb7de9f0d02a41cfecd9109e253d7fd3529e27467dee4254faa67f3ac21451` |

A mono file draws with `fill="currentColor"`, so GPUI's `svg()` tints it with the text color like the icons in `assets/hugeicons/`. A color file keeps its own colors and needs `img()`, because `svg()` keeps only the alpha. OpenRouter uses the mono file because its color one is lime, which is hard to see on a light background.

The edits made to each download:

- The root `width="1em"` and `height="1em"` became `width="64"` and `height="64"`. The `viewBox="0 0 24 24"` is unchanged.
- The root `style` attribute is gone.
- The `<title>` element is gone. In `xai.svg` it wrongly said Grok.
- Each file ends with a newline.

The paths, gradients and colors are unchanged.

The OpenAI-compatible server has no logo, because it stands for any server, such as Ollama, LM Studio or a Whisper server. Settings shows one of Crumble's own icons for it instead.
