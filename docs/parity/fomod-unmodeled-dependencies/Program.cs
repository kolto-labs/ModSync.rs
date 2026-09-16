using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core.Services.Fomod;

var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var candidate = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(candidate) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(candidate) : null;
};

const string xml = "<config><installSteps><installStep name='First'/><installStep name='Visible'><visible>"
    + "<fileDependency file='plugin.esp' state='Active'/><gameDependency version='1.2'/></visible></installStep>"
    + "</installSteps><conditionalFileInstalls><patterns><pattern><dependencies>"
    + "<fileDependency file='plugin.esp' state='Missing'/><gameDependency version='1.3'/></dependencies>"
    + "<files><file source='conditional'/></files></pattern></patterns></conditionalFileInstalls></config>";
var config = FomodParser.ParseModuleConfigXml(xml);
var flags = new Dictionary<string, string>();
Console.WriteLine(JsonSerializer.Serialize(new {
    Visible = FomodToComponentMapper.EvaluateDependency(config.InstallSteps[1].Visible, flags),
    Conditional = FomodToComponentMapper.EvaluateDependency(config.ConditionalInstallPatterns[0].Dependencies, flags),
    VisibleKinds = config.InstallSteps[1].Visible.Children.Select(child => child.Type.ToString()),
    ConditionalKinds = config.ConditionalInstallPatterns[0].Dependencies.Children.Select(child => child.Type.ToString()),
}, new JsonSerializerOptions { WriteIndented = true }));
