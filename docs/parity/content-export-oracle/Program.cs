using System.Runtime.Loader;
using System.Text.Json;
var reference=args[0];
AssemblyLoadContext.Default.Resolving += (_,name)=> {var path=Path.Combine(reference,name.Name+".dll"); return File.Exists(path)?AssemblyLoadContext.Default.LoadFromAssemblyPath(path):null;};
Run(args[1]);
static void Run(string root) {
 var results=new List<object>();
 foreach(var format in new[]{"toml","yaml"}) {
  ModSync.Core.MainConfig.PreambleContent="SENTINEL";
  ModSync.Core.MainConfig.EpilogueContent="SENTINEL";
  ModSync.Core.MainConfig.WidescreenWarningContent="SENTINEL";
  var components=ModSync.Core.Services.ModComponentSerializationService.DeserializeModComponentFromString(File.ReadAllText(Path.Combine(root,"guide."+format)),format);
  results.Add(new {Format=format,Count=components.Count,ModSync.Core.MainConfig.PreambleContent,ModSync.Core.MainConfig.EpilogueContent,ModSync.Core.MainConfig.WidescreenWarningContent});
 }
 File.WriteAllText(Path.Combine(root,"result.json"),JsonSerializer.Serialize(results,new JsonSerializerOptions{WriteIndented=true}));
}
