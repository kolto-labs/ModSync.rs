# Rich authored content and component summary

Working source, 2026-10-02. Implementation and independent source review do not
establish native rendering, focus, screen-reader or platform acceptance.

`ui/rich_content.rs` is a presentation-only CommonMark event adapter. It renders
paragraphs, headings, nested lists/quotes, emphasis, strong, strike, code, line
breaks and ordinary web links. No HTML execution, image loading, guide parsing
or remote content retrieval occurs. Links require explicit activation and reject
credentials and non-HTTP(S) schemes. The destination is available on hover.

Unsupported constructs switch the entire authored section to literal source
with a visible explanation rather than dropping parts of it. The original text
remains copyable. Parsing is capped at 1 MiB and 32,768 events, with a cached
document keyed by the caller's stable surface identity and exact source text.
Larger content stays available through Copy source text. Rich layout is not a
claim of pixel-identical Markdown rendering.

The desktop-only dependency is pinned `pulldown-cmark = 0.13.4`, without default
features. Its published manifest and downloaded source identify MIT licensing
and Rust 1.71.1 minimum; the event/tag API was inspected in the exact locked
source. See the [upstream package source](https://docs.rs/crate/pulldown-cmark/0.13.4/source/Cargo.toml.orig).
Dependency retrieval did not execute project code or tests.

`ui/summary_page.rs` presents full component information, method/language/tier,
categories, platform markers, authored rich-content sections, warnings, download
links, option status and instruction details. Normal description uses the full
authored text, not the short selection-list excerpt. Names and descriptions use
the existing shared spoiler projection; authored spoiler alternatives and the
pinned reference's generic directions/download/warning fallback are retained.
URL and instruction disclosure have explicit spoiler-mode expanders, whose state
is separate from the normal mode. Images remain unfetched source content.

The reference contracts inspected were `SummaryTab.axaml` and
`SpoilerFreeContentConverter.axaml.cs` at pinned C# commit `d31ccbd3`.
Warning fields without a reference spoiler projection retain their authored
content. This does not guarantee that an author's warnings contain no spoilers.

Summary returns typed presentation actions. Component download remains routed
through the existing acquisition owner and is disabled for an unselected
component. Editor navigation identifies the component and instruction; it does
not edit the running installation or replace the guide. Actual option selection
remains with the existing shared selection adapter.

Wizard Preamble uses the rich renderer. Aspyr and Widescreen notices display
literal, wrapped warning text with an explicit copy action, matching the pinned
reference notice contract; Markdown-looking warning text is not interpreted.

The main-window Help, About and More menus now preserve the pinned
`MenuBuilderService.cs` destinations and nested project groups, including the
DeadlyStream project/tool links, Holocron Toolset Discord, Auto-Translate / Font
Creator and KotorDiff. Other Projects belongs under More. The reference's
clickable PyKotor parent also has children; the egui menu retains its destination
as a named child link. Links open only on explicit activation. This source change
does not claim that external destinations were checked or that native menu
keyboard behavior was exercised.

Native behavior and the complete shell/accessibility acceptance checklist remain
separate work. These corrections were reviewed from source only; no tests,
application launch or native probes were performed.
