using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core;
using ModSync.Core.Services.Conflicts;
var reference=args[0];
AssemblyLoadContext.Default.Resolving += (_,name)=> {var p=Path.Combine(reference,name.Name+".dll");return File.Exists(p)?AssemblyLoadContext.Default.LoadFromAssemblyPath(p):null;};
await Run(args[1]);
static async Task Run(string root) {
 var mods=Path.Combine(root,"mods");var game=Path.Combine(root,"game");
 Directory.CreateDirectory(Path.Combine(mods,"a"));Directory.CreateDirectory(Path.Combine(mods,"b"));Directory.CreateDirectory(game);
 File.WriteAllText(Path.Combine(mods,"a","asset.txt"),"first");File.WriteAllText(Path.Combine(mods,"b","asset.txt"),"last");
 MainConfig.Instance=new MainConfig {sourcePath=new DirectoryInfo(mods),destinationPath=new DirectoryInfo(game)};
 Instruction Copy(string source,string dest,bool overwrite) => new Instruction {Action=Instruction.ActionType.Copy,Source=new List<string>{source},Destination=dest,Overwrite=overwrite};
 var components=new List<ModComponent>{
 new ModComponent {Guid=Guid.Parse("11111111-1111-1111-1111-111111111111"),Name="First",IsSelected=true,Instructions=new System.Collections.ObjectModel.ObservableCollection<Instruction>{Copy("<<modDirectory>>/a/asset.txt","<<kotorDirectory>>/Override",true),Copy("<<modDirectory>>/a/asset.txt","<<kotorDirectory>>/Override",true),Copy("<<modDirectory>>/a/asset.txt","<<modDirectory>>/staging",true)}},
 new ModComponent {Guid=Guid.Parse("22222222-2222-2222-2222-222222222222"),Name="Skipped overwrite",IsSelected=true,Instructions=new System.Collections.ObjectModel.ObservableCollection<Instruction>{Copy("<<modDirectory>>/b/asset.txt","<<kotorDirectory>>/Override",false)}},
 new ModComponent {Guid=Guid.Parse("33333333-3333-3333-3333-333333333333"),Name="Last",IsSelected=true,Instructions=new System.Collections.ObjectModel.ObservableCollection<Instruction>{Copy("<<modDirectory>>/b/asset.txt","<<kotorDirectory>>/Override",true),Copy("<<modDirectory>>/b/asset.txt","<<modDirectory>>/staging",true)}}};
 var result=await new FileConflictAnalyzer().AnalyzeAsync(components);
 File.WriteAllText(Path.Combine(root,"result.json"),JsonSerializer.Serialize(result,new JsonSerializerOptions{WriteIndented=true}));
 if(Directory.GetFiles(game,"*",SearchOption.AllDirectories).Length!=0 || Directory.Exists(Path.Combine(mods,"staging"))) throw new Exception("Analysis wrote install trees");
}
