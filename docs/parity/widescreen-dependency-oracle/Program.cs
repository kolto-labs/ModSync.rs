using System.Runtime.Loader;
using System.Text.Json;
using System.Reflection;
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => { var path = Path.Combine(reference, name.Name + ".dll"); return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null; };
await Run(args[1]);
static async Task Run(string root) {
 var mods=Path.Combine(root,"mods"); var game=Path.Combine(root,"game"); Directory.CreateDirectory(mods); Directory.CreateDirectory(game);
 ModSync.Core.MainConfig.Instance = new ModSync.Core.MainConfig { sourcePath=new DirectoryInfo(mods), destinationPath=new DirectoryInfo(game) };
 typeof(ModSync.Core.MainConfig).GetProperty("NoCheckpoint").SetValue(null,true);
 File.WriteAllText(Path.Combine(mods,"base.txt"),"base"); File.WriteAllText(Path.Combine(mods,"wide.txt"),"first");
 var raw = JsonSerializer.Serialize(new[]{new {Guid="11111111-1111-4111-8111-000000000001",Name="Base",IsSelected=true,WidescreenOnly=false,Instructions=new[]{new {Action="Copy",Source=new[]{"<<modDirectory>>/base.txt"},Destination="<<kotorDirectory>>/Override",Overwrite=true}}},new {Guid="11111111-1111-4111-8111-000000000002",Name="Wide",IsSelected=false,WidescreenOnly=true,Instructions=new[]{new {Action="Copy",Source=new[]{"<<modDirectory>>/wide.txt"},Destination="<<kotorDirectory>>/Override",Overwrite=true}}}});
 var components=ModSync.Core.Services.ModComponentSerializationService.DeserializeModComponentFromString(raw,"json");
 components[1].Dependencies = new List<Guid>{components[0].Guid};
 var bulk=await ModSync.Core.Services.InstallationService.InstallAllSelectedComponentsAsync(components.ToList(),cancellationToken:CancellationToken.None);
 var path=Path.Combine(game,"Override","wide.txt"); var afterBulk=File.Exists(path)?File.ReadAllText(path):null; var stateAfterBulk=components[1].InstallState.ToString();
 components[1].IsSelected = true;
 File.WriteAllText(Path.Combine(mods,"wide.txt"),"second");
 var single=await ModSync.Core.Services.InstallationService.InstallSingleComponentAsync(components[1],new[]{components[1]},CancellationToken.None);
 var afterNarrow=File.Exists(path)?File.ReadAllText(path):null;
 var full=await ModSync.Core.Services.InstallationService.InstallSingleComponentAsync(components[1],components.ToList(),CancellationToken.None);
 File.WriteAllText(Path.Combine(root,"result.json"),JsonSerializer.Serialize(new {Bulk=bulk.ToString(),AfterBulk=afterBulk,StateAfterBulk=stateAfterBulk,Narrow=single.ToString(),AfterNarrow=afterNarrow,Full=full.ToString(),AfterFull=File.Exists(path)?File.ReadAllText(path):null},new JsonSerializerOptions{WriteIndented=true}));
}
