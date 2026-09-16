using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core.Services.Fomod;

var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};

const string xml = "<config><installSteps><installStep name='Types'><optionalFileGroups><group name='All types' type='SelectAny'><plugins>"
    + "<plugin name='Optional'><typeDescriptor><type name='Optional'/></typeDescriptor></plugin>"
    + "<plugin name='Required'><typeDescriptor><type name='Required'/></typeDescriptor></plugin>"
    + "<plugin name='Recommended'><typeDescriptor><dependencyType><defaultType name='Recommended'/></dependencyType></typeDescriptor></plugin>"
    + "<plugin name='Blocked'><typeDescriptor><type name='NotUsable'/></typeDescriptor></plugin>"
    + "<plugin name='Conditional'><typeDescriptor><type name='CouldBeUsable'/></typeDescriptor></plugin>"
    + "<plugin name='Unknown'><typeDescriptor><type name='Other'/></typeDescriptor></plugin>"
    + "</plugins></group></optionalFileGroups></installStep></installSteps></config>";
var config = FomodParser.ParseModuleConfigXml(xml);
Console.WriteLine(JsonSerializer.Serialize(config.InstallSteps[0].Groups[0].Plugins.Select(plugin => new {
    plugin.Name,
    Type = plugin.TypeDescriptor.ToString(),
}), new JsonSerializerOptions { WriteIndented = true }));
