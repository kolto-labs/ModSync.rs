using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core.Services.Fomod;

var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};

var config = FomodParser.ParseModuleConfigXml(
    "<config><requiredInstallFiles><file source='textures/a.tga' destination='Override/b.tga'/></requiredInstallFiles></config>");
var instruction = FomodToComponentMapper.Map(null, config, "package.zip").Instructions[0];
Console.WriteLine(JsonSerializer.Serialize(new { instruction.Action, instruction.Source, instruction.Destination }, new JsonSerializerOptions { WriteIndented = true }));
