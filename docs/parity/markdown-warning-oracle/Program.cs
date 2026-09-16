using System.Runtime.Loader;
using System.Text.Json;
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => { var path = Path.Combine(reference, name.Name + ".dll"); return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null; };
Run(args[1]);
static void Run(string root) {
 var parsed = new ModSync.Core.Parsing.MarkdownParser(ModSync.Core.Parsing.MarkdownImportProfile.CreateDefault()).Parse(File.ReadAllText(Path.Combine(root,"guide.md")));
 File.WriteAllText(Path.Combine(root,"result.json"),JsonSerializer.Serialize(new {parsed.WidescreenWarningContent, parsed.EpilogueContent, Components=parsed.Components.Select(c=>new {c.Name,c.WidescreenOnly})},new JsonSerializerOptions{WriteIndented=true}));
}
