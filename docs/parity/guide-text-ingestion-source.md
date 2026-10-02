# Bounded guide text ingestion

Source-only correction in `model::text` and the shared `Guide` file loader.
No tests, fixtures, runtime probes or builds were executed for this slice.

## Reference behavior

Pinned C# `FileLoadingService.cs:40,100` calls `ReadFileWithEncodingFallback`.
Its primary path at lines 200–209 reads bytes, decodes with nonthrowing UTF-8,
and replaces every U+FFFD character with `_`. Malformed UTF-8 does not throw,
so it does not reach the catch fallback's `File.ReadAllText` at line 215.
That fallback's BOM-aware behavior is not the normal guide-loader contract.

`ModComponentSerializationService.SanitizeUtf8` tests each character against a
configured encoder and retains the original character after successful encoding;
it is not byte transcoding. This slice does not change encoding preferences.

## Rust scope

`Guide::load`, `load_preserving_selection`, and the file-loading portion of
`load_for_install` now use an ordinary-file reader with an actual 8 MiB byte
limit. The reader uses the existing shared opener, which rejects final symlinks,
reparse points where supported, and special files instead of blocking on a FIFO.
The limit is enforced while reading, including if the file grows after opening.

UTF-8 lossy decoding and U+FFFD-to-underscore replacement remain unchanged.
The existing parser strips UTF-8 BOM characters from decoded text. Format
selection, parsing, selection defaults and prose-draft behavior are unchanged.
UTF-16/32 decoding was not added: acceptance of such input is not established by
the reference primary path, and this source review does not claim it.

`editor::read_document` remains strict UTF-8 because operation lists, preview
JSON and regex profiles share that reader. Editor strict-input behavior, settings
readers and JSON operation contracts are unchanged. Existing parser APIs accepting
an in-memory string are outside this file admission bound.

The desktop File/Open and dropped-guide routes now delegate to the same
`Guide::load_for_install` entry point as ordinary application loading. This removes
the shell's strict-UTF-8/direct-extension duplicate parser, preserves load-time
selection defaults and Markdown draft generation, and applies the shared extension
alias/autodetection rules. Editor operation documents retain their separate strict
reader. This wiring correction is source-reviewed only.
