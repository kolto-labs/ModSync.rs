"""Compare reference download filename selection with Rust CLI plans."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from xml.sax.saxutils import escape

PROBE = r'''using System.Runtime.Loader;
using System.Text.Json;
using System.Reflection;
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => { var path = Path.Combine(reference, name.Name + ".dll"); return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null; };
Run(args.Skip(1).ToArray());
static void Run(string[] args) {
using var input = JsonDocument.Parse(File.ReadAllText(args[0]));
var output = new List<object>();
foreach (var item in input.RootElement.EnumerateArray()) {
 var raw = item.GetProperty("Component");
 var component = ModSync.Core.Services.ModComponentSerializationService.DeserializeModComponentFromString("["+raw.GetRawText()+"]", "json").Single();
 var registry = JsonSerializer.Deserialize<Dictionary<string,ModSync.Core.ResourceMetadata>>(raw.GetProperty("ResourceRegistry").GetRawText());
 component.ResourceRegistry = new Dictionary<string,ModSync.Core.ResourceMetadata>(registry, StringComparer.OrdinalIgnoreCase);
 foreach(var meta in component.ResourceRegistry.Values) meta.Files = new Dictionary<string,bool?>(meta.Files, StringComparer.OrdinalIgnoreCase);
 void Sources(IEnumerable<ModSync.Core.Instruction> steps, JsonElement array) {
 foreach(var (step, value) in steps.Zip(array.EnumerateArray())) step.Source = value.GetProperty("Source").EnumerateArray().Select(s=>s.GetString()).ToList();
 }
 Sources(component.Instructions, raw.GetProperty("Instructions"));
 if(raw.TryGetProperty("Options", out var options)) foreach(var (option,value) in component.Options.Zip(options.EnumerateArray())) Sources(option.Instructions,value.GetProperty("Instructions"));
 var name = item.GetProperty("Name").GetString();
 var directory = Path.Combine(args[1],name);
 var config = new ModSync.Core.MainConfig { sourcePath = item.GetProperty("SourceConfigured").GetBoolean() ? new DirectoryInfo(directory) : null };
 var url = item.GetProperty("Url").GetString();
 var candidates = item.GetProperty("Candidates").EnumerateArray().Select(v=>v.GetString()).ToList();
 var enabled = (bool)typeof(ModSync.Core.Services.DownloadCacheService).GetMethod("ShouldDownloadUrl",BindingFlags.NonPublic|BindingFlags.Static).Invoke(null,new object[]{component,url});
 try {
 var filenames = ModSync.Core.Services.DownloadCacheService.GetFilenamesForDownload(component,url,candidates);
 output.Add(new {Name=name, UrlEnabled=enabled,Filenames=filenames,Error=(string)null});
 } catch(Exception error) {output.Add(new {Name=name,UrlEnabled=enabled,Filenames=Array.Empty<string>(),Error=error.Message});}
}
File.WriteAllText(args[2],JsonSerializer.Serialize(output,new JsonSerializerOptions{WriteIndented=true}));
}
'''

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll', type=Path, required=True)
parser.add_argument('--rust', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
assert shutil.which('bwrap'), 'bubblewrap is required to isolate the reference'
root = Path(__file__).resolve().parents[1]
fixture = root / 'tests/fixtures/download-plan.json'
cases = json.loads(fixture.read_text())
reference = args.reference_dll.resolve(strict=True)
rust = args.rust.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
source = fixture
for case in cases:
    directory = output / case['Name']
    directory.mkdir(exist_ok=True)
    for filename in case.get('DiskFiles', []):
        target = directory / filename
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('fixture')
probe = output / 'probe'
probe.mkdir(exist_ok=True)
(probe / 'probe.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net9.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="ModSync.Core"><HintPath>' + escape(str(reference)) + '</HintPath><Private>false</Private></Reference></ItemGroup></Project>')
(probe / 'Program.cs').write_text(PROBE)

def run(command, label):
    result = subprocess.run(command, cwd=output, capture_output=True, timeout=300)
    (output / (label + '.log')).write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, f'{label} failed; see {output}/{label}.log'

run(['dotnet', 'build', str(probe / 'probe.csproj'), '-v', 'quiet'], 'build-probe')
sandbox = ['bwrap', '--ro-bind', '/', '/', '--dev', '/dev', '--tmpfs', '/tmp',
           '--tmpfs', str(Path.home() / '.local/share'), '--tmpfs', str(Path.home() / '.config'),
           '--bind', str(output), str(output), '--unshare-net', '--die-with-parent']
expected_path = output / 'csharp.json'
run(sandbox + ['dotnet', str(probe / 'bin/Debug/net9.0/probe.dll'), str(reference.parent), str(source), str(output), str(expected_path)], 'csharp')
expected = json.loads(expected_path.read_text())
actual = []
for case in cases:
    guide = output / (case['Name'] + '.json')
    guide.write_text(json.dumps([case['Component']]))
    command = [str(rust), 'download-plan', str(guide), '--component', case['Component']['Guid'], '--url', case['Url']]
    for filename in case['Candidates']: command.extend(['--filename', filename])
    if case['SourceConfigured']: command.extend(['--source-dir', str(output / case['Name'])])
    result = subprocess.run(command, capture_output=True, timeout=60)
    (output / (case['Name'] + '.log')).write_bytes(result.stdout+result.stderr)
    if result.returncode:
        assert result.returncode == 1
        actual.append({'Name':case['Name'], 'UrlEnabled': None, 'Filenames':[], 'Error':result.stderr.decode().strip().removeprefix('Error: ')})
    else: actual.append({'Name':case['Name'], **json.loads(result.stdout), 'Error':None})
(output / 'rust.json').write_text(json.dumps(actual,indent=2)+'\n')
receipt = {'reference_commit':'d31ccbd3bfcddb4705917eafa6483715c166de3b','reference_dll_sha256':hashlib.sha256(reference.read_bytes()).hexdigest(),'fixture_sha256':hashlib.sha256(fixture.read_bytes()).hexdigest(),'cases':len(cases),'differences':[{'csharp':a,'rust':b} for a,b in zip(expected,actual) if ({k:v for k,v in a.items() if k != 'UrlEnabled'} != {k:v for k,v in b.items() if k != 'UrlEnabled'} if b['Error'] else a!=b)], 'scope':'Direct C# consumer with raw file selections and Source strings restored after loading. Rust CLI plans on isolated disk fixtures. Error cases compare error text only, not URL-enabled result. Not host acquisition, UI, cache persistence or full readiness validation.'}
(output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt,indent=2))
assert len(expected)==len(actual)==len(cases)
assert not receipt['differences']
