using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core;
using ModSync.Core.Services.FileSystem;
using ModSync.Core.Services.Validation;

var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) =>
{
    var path = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null;
};

var component = new ModComponent
{
    Guid = Guid.Parse("11111111-1111-1111-1111-111111111111"),
    Name = "Fixture",
};
var dryRun = new DryRunValidationResult
{
    Issues = new List<ValidationIssue>
    {
        new()
        {
            Severity = ValidationSeverity.Error,
            Category = "CopyFile",
            Message = "Source file does not exist",
            AffectedPath = "/mods/missing.tga",
            AffectedComponent = component,
            InstructionIndex = 2,
            Solution = "Restore the source file",
        },
        new()
        {
            Severity = ValidationSeverity.Warning,
            Category = "DeleteFile",
            Message = "Attempting to delete non-existent file",
            AffectedPath = "/game/missing.tga",
            AffectedComponent = component,
        },
    },
};
var pipeline = new ValidationPipelineResult
{
    IsSuccess = false,
    HasCriticalErrors = true,
    ErrorCount = 1,
    WarningCount = 1,
    PassedCount = 1,
    DryRunResult = dryRun,
};
pipeline.Stages.Add(new ValidationPipelineStageResult
{
    Stage = ValidationPipelineStage.Environment,
    Passed = true,
    Summary = "Passed",
});
var dryRunStage = new ValidationPipelineStageResult
{
    Stage = ValidationPipelineStage.DryRun,
    Passed = false,
    HasWarnings = true,
    Summary = "Dry-run validation failed.",
};
dryRunStage.Messages.Add("ERROR: [CopyFile] Source file does not exist");
pipeline.Stages.Add(dryRunStage);

var result = new Dictionary<string, JsonElement>
{
    ["report"] = JsonSerializer.Deserialize<JsonElement>(
        ValidationPipelineJsonFormatter.SerializeReport(pipeline, 1, "/guides/fixture.json")),
    ["error"] = JsonSerializer.Deserialize<JsonElement>(
        ValidationPipelineJsonFormatter.SerializeError("fixture failure", 7)),
};
File.WriteAllText(
    Path.Combine(args[1], "result.json"),
    JsonSerializer.Serialize(result, new JsonSerializerOptions { WriteIndented = true }) + Environment.NewLine);
