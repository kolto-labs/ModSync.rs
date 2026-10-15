using ModSync.Core.Services;
using System.Text.Json;
var res = new Dictionary<string,string>();
try {
var fmt = args[0]; var text = File.ReadAllText(args[1]);
var comps = fmt == "toml" ? ModComponentSerializationService.DeserializeModComponentFromTomlString(text)
                          : ModComponentSerializationService.DeserializeModComponentFromYamlString(text);
var lm = ModSync.Core.MainConfig.LastModified;
res["value"] = lm.HasValue ? lm.Value.ToString("O") : null;
res["kind"] = lm.HasValue ? lm.Value.Kind.ToString() : null;
foreach (var o in args.Skip(2)) res[o] = ModComponentSerializationService.SerializeModComponentAsString(comps, o);
} catch (Exception e) { res["error"] = e.GetType().Name + ": " + e.Message.Split('\n')[0]; }
Console.WriteLine("@@JSON@@" + JsonSerializer.Serialize(res));
