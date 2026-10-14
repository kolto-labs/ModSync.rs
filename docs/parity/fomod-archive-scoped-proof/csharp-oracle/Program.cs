using System.IO.Compression;
using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core;
using ModSync.Core.Services.Fomod;

// args: <reference-dir> <modDirectory>...  Each mod directory holds empty.zip.
// Emulates C#'s configure flow (FomodPostDownloadOrchestrator): map the archive's
// ModuleConfig.xml, merge the configured component, mark it configured, then gate.
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};
var rows = new Dictionary<string, object>();
foreach (var modDirectory in args.Skip(1)) rows[modDirectory] = Run(modDirectory);
Console.WriteLine(JsonSerializer.Serialize(rows, new JsonSerializerOptions { WriteIndented = true }));

static object Run(string modDirectory)
{
    const string archive = "empty.zip";
    string xml;
    using (var zip = ZipFile.OpenRead(Path.Combine(modDirectory, archive)))
    using (var reader = new StreamReader(zip.GetEntry("fomod/ModuleConfig.xml")!.Open()))
        xml = reader.ReadToEnd();
    var component = new ModComponent { Guid = Guid.Parse("aaaaaaaa-1111-4111-8111-111111111111"), Name = "Example Consumer", IsSelected = true };
    component.ResourceRegistry = new Dictionary<string, ResourceMetadata> {
        ["https://example.invalid/empty.zip"] = new ResourceMetadata { Files = new Dictionary<string, bool?> { [archive] = true } },
    };
    var configured = FomodToComponentMapper.Map(null, FomodParser.ParseModuleConfigXml(xml), archive);
    FomodConfiguredComponentMerger.MergeInto(component, configured, archive);
    FomodDownloadPromptState.MarkConfigured(component, archive);
    var result = FomodConfigurationGate.Validate(new[] { component }, new[] { component }, modDirectory);
    return new {
        Status = FomodDownloadPromptState.GetStatus(component, archive),
        ArchiveScoped = FomodConfiguredComponentMerger.HasArchiveScopedInstructions(component, archive),
        Passed = result.Passed,
        Warnings = result.Warnings.Select(w => $"WARNING: {w.Component.Name}: {w.Message}"),
        Issues = result.Issues.Select(i => $"ERROR: {i.Component.Name}: {FomodConfigurationGate.FormatIssueMessage(i)}"),
    };
}
