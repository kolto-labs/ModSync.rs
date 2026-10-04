using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core;
using ModSync.Core.Services;
using ModSync.Core.Services.Validation;
using ModSync.Core.Services.FileSystem;

// args: <reference-dir> <load|dryrun|install> <guide> <modDirectory> <kotorDirectory>
// The guide is loaded with FileLoadingService.LoadFromFile (any format, as the
// reference CLI and GUI load it).
// load: each component's instructions (Action, Source) and options (Guid as
// Guid.ToString(), IsSelected), as the reference holds them after loading.
// dryrun: DryRunValidator.ValidateInstallationAsync over the loaded components.
// Before dryrun or install, a guide with no selected component has every component
// selected. install: ModComponent.InstallAsync per selected component in guide order, stopping
// at the first non-success exit as the reference installer does.
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};
Console.WriteLine("@@JSON@@" + JsonSerializer.Serialize(await Run(args[1], args[2], args[3], args[4])));

static async Task<object> Run(string mode, string guide, string mods, string game)
{
    var config = new MainConfig { sourcePath = new DirectoryInfo(mods), destinationPath = new DirectoryInfo(game) };
    var components = FileLoadingService.LoadFromFile(guide).ToList();
    if (mode == "load")
    {
        return components.Select(c => new {
            Instructions = c.Instructions.Select(i => new { Action = i.Action.ToString(), Source = i.Source }),
            Options = c.Options.Select(o => new { Guid = o.Guid.ToString(), o.IsSelected }),
        });
    }
    // A Markdown guide loads its components unselected; select them all, as the Rust
    // CLI does when no component is selected, so both sides run the same component.
    if (!components.Any(c => c.IsSelected))
    {
        components.ForEach(c => c.IsSelected = true);
    }
    if (mode == "dryrun")
    {
        var result = await DryRunValidator.ValidateInstallationAsync(components);
        return new {
            Errors = result.Issues.Where(i => i.Severity == ValidationSeverity.Error).Select(i => i.Message),
            Warnings = result.Issues.Where(i => i.Severity == ValidationSeverity.Warning).Select(i => i.Message),
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
