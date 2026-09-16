These files were generated from `../guide.json` by the C# `ModSync.Core` CLI at
commit `d31ccbd3bfcddb4705917eafa6483715c166de3b` using `convert --input ...
--output ... --format <format> --plaintext --non-interactive` on .NET 9.
The source was built with `dotnet build src/ModSync.Core/ModSync.Core.csproj
--configuration Debug --framework net9.0 --artifacts-path <scratch-build>`.

Generation ran inside bubblewrap with a read-only host filesystem, writable
scratch output, private `/dev`, `/tmp`, application data and settings, and no
network. These fixtures capture the reference serializer's actual output,
including its defaults and metadata transformations. They do not assert that
all reference transformations are lossless. `tools/check_formats.py` provides
the live bidirectional check; `docs/parity/format-check.json` records its proof
boundary and the reference DLL hash.
