using System.Runtime.Loader;
using System.Text.Json;
using ModSync.Core.Services.Fomod;
var reference=args[0];
AssemblyLoadContext.Default.Resolving += (_,name)=> {var path=Path.Combine(reference,name.Name+".dll");return File.Exists(path)?AssemblyLoadContext.Default.LoadFromAssemblyPath(path):null;};
Run(args[1]);
static void Run(string root) {
 var results=new List<object>();
 foreach(var group in new[]{"SelectAny","SelectExactlyOne","SelectAtMostOne","SelectAtLeastOne","SelectAll"}) {
  var session=Session(group,"Optional");
  var defaults=session.Steps[0].Groups[0].Plugins.Select(p=>p.IsSelected).ToArray();
  var initialValidation=FomodInstallerPresenter.ValidateStep(session,0);
  var clear=FomodInstallerPresenter.TrySetPluginSelected(session,0,0,0,false);
  var clearedValidation=FomodInstallerPresenter.ValidateStep(session,0);
  var select=FomodInstallerPresenter.TrySetPluginSelected(session,0,0,1,true);
  FomodInstallerPresenter.ApplySelectionsToComponent(session);
  results.Add(new {Group=group,Defaults=defaults,InitialValidation=initialValidation,Clear=clear,ClearedValidation=clearedValidation,Select=select,Final=session.Component.Options.Select(o=>o.IsSelected).ToArray(),FinalValidation=FomodInstallerPresenter.ValidateStep(session,0)});
 }
 var required=Session("SelectExactlyOne","Required");
 var direct=FomodInstallerPresenter.TrySetPluginSelected(required,0,0,0,false);
 var sibling=FomodInstallerPresenter.TrySetPluginSelected(required,0,0,1,true);
 File.WriteAllText(Path.Combine(root,"result.json"),JsonSerializer.Serialize(new {Groups=results,Required=new{DirectClear=direct,SelectSibling=sibling,Selected=required.Steps[0].Groups[0].Plugins.Select(p=>p.IsSelected).ToArray(),Validation=FomodInstallerPresenter.ValidateStep(required,0)}},new JsonSerializerOptions{WriteIndented=true}));
}
static FomodInstallerSession Session(string group,string firstType) {
 var xml=$"<config><moduleName>Probe</moduleName><installSteps><installStep name='Step'><optionalFileGroups><group name='Group' type='{group}'><plugins><plugin name='First'><typeDescriptor><type name='{firstType}'/></typeDescriptor></plugin><plugin name='Second'><typeDescriptor><type name='Optional'/></typeDescriptor></plugin></plugins></group></optionalFileGroups></installStep></installSteps></config>";
 var config=FomodParser.ParseModuleConfigXml(xml);
 return FomodInstallerPresenter.CreateSession(config,FomodToComponentMapper.Map(null,config,"probe.zip"));
}
