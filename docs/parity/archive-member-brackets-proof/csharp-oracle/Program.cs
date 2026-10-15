using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core;
using ModSync.Core.Services;
using ModSync.Core.Services.Validation;
using ModSync.Core.Services.FileSystem;

// args: <reference-dir> <dryrun|install|pipeline> <guide.toml> <modDirectory> <kotorDirectory>
// dryrun: DryRunValidator.ValidateInstallationAsync over the selected components.
// install: ModComponent.InstallAsync per selected component in guide order, stopping
// at the first non-success exit as the reference installer does.
// pipeline: InstallationValidationPipeline.RunAsync with DryRun and file selection
// (the wizard preset without the environment and conflict stages), reporting each
// stage's Passed flag and messages.
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};
Console.WriteLine("@@JSON@@" + JsonSerializer.Serialize(await Run(args[1], args[2], args[3], args[4])));

static async Task<object> Run(string mode, string guide, string mods, string game)
{
    var config = new MainConfig { sourcePath = new DirectoryInfo(mods), destinationPath = new DirectoryInfo(game) };
    var components = ModComponentSerializationService.DeserializeModComponentFromTomlString(File.ReadAllText(guide)).ToList();
    if (mode == "dryrun")
    {
        var result = await DryRunValidator.ValidateInstallationAsync(components);
        return new {
            Errors = result.Issues.Where(i => i.Severity == ValidationSeverity.Error).Select(i => i.Message),
            Warnings = result.Issues.Where(i => i.Severity == ValidationSeverity.Warning).Select(i => i.Message),
        };
    }
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
    var exits = new List<string>();
    foreach (var component in components.Where(c => c.IsSelected))
    {
        var exit = await component.InstallAsync(components, CancellationToken.None);
        exits.Add($"{component.Name}: {exit}");
        if (exit != ModComponent.InstallExitCode.Success) break;
    }
    return new { Exits = exits };
}
