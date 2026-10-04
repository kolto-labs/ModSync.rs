using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core;
using ModSync.Core.Services;
using ModSync.Core.Services.Conflicts;
using ModSync.Core.Services.Validation;
using ModSync.Core.Services.FileSystem;

// args: <reference-dir> <pipeline|conflicts> <guide.toml> <modDirectory> <kotorDirectory>
// pipeline: InstallationValidationPipeline.RunAsync with DryRun and file selection
// (the wizard preset without the environment and conflict stages), reporting each
// stage's Passed flag and messages, and the dry-run issues.
// conflicts: FileConflictAnalyzer.AnalyzeAsync over the guide's components in guide
// order, as the wizard's conflict check runs it.
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};
Console.WriteLine("@@JSON@@" + JsonSerializer.Serialize(await Run(args[1], args[2], args[3], args[4])));

static async Task<object> Run(string mode, string guide, string mods, string game)
{
    var config = new MainConfig { sourcePath = new DirectoryInfo(mods), destinationPath = new DirectoryInfo(game) };
    MainConfig.Instance = config;
    var components = ModComponentSerializationService.DeserializeModComponentFromTomlString(File.ReadAllText(guide)).ToList();
    if (mode == "pipeline")
    {
        var options = new ValidationPipelineOptions { DryRun = true, UseFileSelection = true, MainConfig = config };
        var result = await InstallationValidationPipeline.RunAsync(components, options);
        return new {
            result.IsSuccess,
            result.ErrorCount,
            Stages = result.Stages.Select(s => new { Stage = s.Stage.ToString(), s.Passed, s.Messages }),
            DryRunErrors = result.DryRunResult?.Issues.Where(i => i.Severity == ValidationSeverity.Error).Select(i => i.Message),
            DryRunWarnings = result.DryRunResult?.Issues.Where(i => i.Severity == ValidationSeverity.Warning).Select(i => i.Message),
        };
    }
    var analysis = await new FileConflictAnalyzer().AnalyzeAsync(components);
    return new {
        Conflicts = analysis.Conflicts.Select(c => new {
            c.DestinationPath,
            Writers = c.Writers.Select(w => new { ComponentGuid = w.ComponentGuid.ToString(), w.ComponentName, w.InstructionIndex }),
            WinnerComponentGuid = c.WinnerComponentGuid.ToString(),
        }),
        ConflictCountsByComponent = analysis.ConflictCountsByComponent.ToDictionary(p => p.Key.ToString(), p => p.Value),
        analysis.AnalyzedComponentCount,
    };
}
