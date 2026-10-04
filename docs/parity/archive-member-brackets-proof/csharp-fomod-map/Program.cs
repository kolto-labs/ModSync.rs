// args: <reference-dir> <ModuleConfig file source> <archive file name>
// Maps a one-file requiredInstallFiles ModuleConfig with FomodToComponentMapper.Map
// and prints each generated instruction, to show how mapped Sources are spelled.
using System.Runtime.Loader;
using ModSync.Core.Services.Fomod;
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var c = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(c) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(c) : null;
};
Run(args[1], args[2]);
static void Run(string src, string archive)
{
    var xml = $"<config><moduleName>T</moduleName><requiredInstallFiles><file source=\"{src}\" destination=\"Override\"/></requiredInstallFiles></config>";
    var component = FomodToComponentMapper.Map(null, FomodParser.ParseModuleConfigXml(xml), archive);
    void Dump(ModSync.Core.ModComponent c) { foreach (var i in c.Instructions) Console.WriteLine($"{i.Action} [{string.Join(",", i.Source)}] -> {i.Destination}"); foreach (var o in c.Options) Dump(o); }
    Dump(component);
}
