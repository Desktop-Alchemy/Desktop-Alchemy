# Translation Files

This directory uses standard UTF-8 nested JSON. `en.json` is the English source; `zh-CN.json` is the Simplified Chinese translation. Every leaf is a string, with no comments, arrays, executable functions, or platform-specific metadata. Use a generic nested-JSON translation filter for import/export.

## Platform settings

| Platform                | Configuration                                                                                                                                                                                          |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Crowdin                 | Select **JSON**, upload `en.json` as source, and import `zh-CN.json` as existing Simplified Chinese translations. Enable single-brace placeholder protection; preserve nesting and string identifiers. |
| Weblate                 | Select **JSON nested structure file**. Use file mask `src/locales/*.json`, monolingual base `src/locales/en.json`, and English as the source language.                                                 |
| Lokalise                | Select **JSON**, preserve nested keys, disable placeholder conversion (`convert_placeholders: false` in the API), and export placeholders in their original format.                                    |
| Other CAT/editing tools | Select a JSON filter supporting nested objects, translate string values only, and preserve keys, nesting, and placeholder syntax on export.                                                            |

Tools supporting only CSV, PO, or XLIFF require format conversion. Native JSON support does not imply every editor recognizes framework-specific syntax. Platform account configuration and remote round trips require separate validation.

## Conventions

- `{name}`, `{count}`, and `{arg0}` are named placeholders. They may move within a sentence but must not be renamed, translated, removed, or converted to `{{name}}`, `%s`, or ICU expressions.
- Plurals use separate strings in a nested object, for example `phrases.appCount.zero / one / other`. The shared translator uses the count and the locale's Intl.PluralRules, falling back to `other`; a declared `zero` branch takes precedence at zero. Do not merge branches into Vue pipe-separated text or let a platform convert them to ICU, arrays, or i18next plural formats.
- Keep keys and hierarchy stable when display wording changes. Preserve the meaning of Desktop Alchemy, model names, protocols, extensions, and other proper names.
- Translate complete sentences while preserving consequences, failure conditions, and accessible names. Put translator context in documentation or platform comments, not in application UI copy.
- Run `pnpm check:i18n` after importing translations. It validates JSON shape, matching keys/parameters, and source references. Adding a language file alone does not add a supported UI language; explicitly register it in the app.

## Format references

- [Crowdin JSON](https://store.crowdin.com/json): nested JSON support; generic JSON does not natively interpret plural syntax.
- [Crowdin placeholder settings](https://support.crowdin.com/project-settings/placeholders/): single-brace protection.
- [Weblate JSON](https://docs.weblate.org/en/latest/formats/json.html): nested structure preservation.
- [Lokalise upload parameters](https://developers.lokalise.com/reference/upload-a-file): disabling placeholder conversion.
