using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core.Services.Fomod;

var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};

const string xml = "<config><installSteps><installStep><optionalFileGroups><group type='SelectAll'><plugins>"
    + "<plugin name='A'><files><file source='a'/></files><conditionFlags><flag name='mode'>yes</flag></conditionFlags></plugin>"
    + "<plugin name='B'><files><file source='b'/></files><conditionFlags><flag name='mode'>yes</flag></conditionFlags></plugin>"
    + "</plugins></group></optionalFileGroups></installStep></installSteps><conditionalFileInstalls><patterns>"
    + "<pattern><dependencies><dependencies/></dependencies><files><file source='nested'/></files></pattern>"
    + "<pattern><dependencies><flagDependency flag='mode' value='missing'/></dependencies><files><file source='missing'/></files></pattern>"
    + "<pattern><files><file source='trailing'/></files></pattern>"
    + "</patterns></conditionalFileInstalls></config>";
var component = FomodToComponentMapper.Map(null, FomodParser.ParseModuleConfigXml(xml), "package.zip");
Console.WriteLine(JsonSerializer.Serialize(new {
    Options = component.Options.Select(option => new { option.Name, Sources = option.Instructions.SelectMany(instruction => instruction.Source) }),
    ComponentSources = component.Instructions.SelectMany(instruction => instruction.Source).Where(source => source.StartsWith("<<modDirectory>>")),
}, new JsonSerializerOptions { WriteIndented = true }));
