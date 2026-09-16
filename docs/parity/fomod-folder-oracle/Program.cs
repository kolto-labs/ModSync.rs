using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core.Services.Fomod;
var reference=args[0];
AssemblyLoadContext.Default.Resolving += (_,name)=> {var p=Path.Combine(reference,name.Name+".dll");return File.Exists(p)?AssemblyLoadContext.Default.LoadFromAssemblyPath(p):null;};
await Run(args[1]);
static async Task Run(string root) {
 var mods=Path.Combine(root,"mods");var game=Path.Combine(root,"game");
 Directory.CreateDirectory(Path.Combine(mods,"package","textures","nested"));Directory.CreateDirectory(game);
 File.WriteAllText(Path.Combine(mods,"package","textures","top.txt"),"top");
 File.WriteAllText(Path.Combine(mods,"package","textures","nested","deep.txt"),"deep");
 ModSync.Core.MainConfig.Instance=new ModSync.Core.MainConfig {sourcePath=new DirectoryInfo(mods),destinationPath=new DirectoryInfo(game)};
 typeof(ModSync.Core.MainConfig).GetProperty("NoCheckpoint").SetValue(null,true);
 var config=FomodParser.ParseModuleConfigXml("<config><moduleName>Folder probe</moduleName><requiredInstallFiles><folder source='textures' destination='Override'/></requiredInstallFiles></config>");
 var component=FomodToComponentMapper.Map(null,config,"package.zip");component.IsSelected=true;
 var result=await ModSync.Core.Services.InstallationService.InstallAllSelectedComponentsAsync(new List<ModSync.Core.ModComponent>{component},cancellationToken:CancellationToken.None);
 File.WriteAllText(Path.Combine(root,"result.json"),JsonSerializer.Serialize(new {Result=result.ToString(),Instructions=component.Instructions.Select(i=>new{Action=i.Action.ToString(),i.Source,i.Destination,i.Overwrite}),Files=Directory.GetFiles(game,"*",SearchOption.AllDirectories).ToDictionary(p=>Path.GetRelativePath(game,p),File.ReadAllText)},new JsonSerializerOptions{WriteIndented=true}));
}
