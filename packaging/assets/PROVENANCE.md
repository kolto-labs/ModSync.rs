# Native icon source provenance

These files are unchanged copies of the existing ModSync application icons. This is local source packaging preparation, not publication or a native package acceptance receipt.

Source repository: https://github.com/th3w1zard1/ModSync

Pinned source commit: `d31ccbd3bfcddb4705917eafa6483715c166de3b`.

| Local file | Source path | Git blob | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| `modsync.ico` | `src/ModSync.GUI/icon53.ico` | `46411f09d3b2b52a7f7cb757cf201269c43ac108` | 4286 | `f259aff084da26c66c791d539c54716dd361df4fd07f242b645d9ee0d55b93e7` |
| `modsync.icns` | `src/ModSync.GUI/icon53.icns` | `e549aab74cfcc0207cef108aad05fe577681fdd2` | 4096 | `af32631e2d126acd576dbc424c9cd112a0b687eb22232d44494b2fea45bbdb3f` |
| `LICENSE.txt` | `LICENSE.txt` | `5f01c782ff92d9245dd1509c9fcc98664c3bd61b` | — | `b8a0f5c6ba0d77d577c2604f78ba8e2849695ca4915d9f18dc5c822df9446715` |

The pinned `src/ModSync.GUI/ModSync.csproj` identifies `icon53.ico` as its ApplicationIcon and embedded resource, and copies `icon53.icns` as application content. Its `Info.plist` identifies `icon53` as the bundle icon. The source files and license had no local changes when copied. No image generation, editing, conversion, resource compilation, application launch or package production was performed.

## License scope

The adjacent `LICENSE.txt` is an unchanged copy of the pinned repository's Business Source License 1.1. Its stated licensor is ModSync, its Licensed Work is ModSync v2.0.0 and later, its Additional Use Grant is None, its Change Date is four years from first public distribution of each version, and its Change License is GNU GPL v3.0 or later. The full license, including its conditions and limitations, remains authoritative.

The source license explicitly excludes rights in the licensor's or affiliates' trademarks and logos, except use expressly required by that license. This provenance record does not assert independent artwork or logo permission, does not describe these assets as permissively licensed, and does not authorize external publication. No separate asset-specific license or attribution was found in the inspected source paths.

For historical provenance only, the two icon Git blobs are also present unchanged at commit `5f79c297bea398ba8ef6ad7633090fdfcfa7fe3b` (2023-10-20), under `KOTORModSync.GUI/`. That snapshot contained a GPLv3 `LICENSE.txt` with Git blob `f288702d2fa16d3cdf0035b15a9fcbc552cd88e7`. This preparation uses the current pinned same-project source and license; it does not select a historical license as a substitute or relicense the artwork.

The manual packaging workflow defaults to these source assets. Digest-bound resource compilation and package staging still perform their normal checks, and native icon/version inspection remains unverified.
