"""Compare legacy resource migration from the pinned C# service and compare Rust."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from xml.sax.saxutils import escape

PROBE = r'''using System.Runtime.Loader;
using System.Text.Json;
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => { var path = Path.Combine(reference, name.Name + ".dll"); return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null; };
Run(args.Skip(1).ToArray());
static void Run(string[] args) {
var components = ModSync.Core.Services.ModComponentSerializationService.DeserializeModComponentFromString(File.ReadAllText(args[0]), "json");
File.WriteAllText(args[1], JsonSerializer.Serialize(components.Select(c => new {c.Name, ResourceRegistry = c.ResourceRegistry.Select(r => new { Url = r.Key, r.Value.ContentKey, r.Value.MetadataHash, r.Value.Files, r.Value.HandlerMetadata, r.Value.FileSize })}), new JsonSerializerOptions {WriteIndented=true}));
}
'''

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll', type=Path, required=True)
parser.add_argument('--rust', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
assert shutil.which('bwrap'), 'bubblewrap is required to isolate the reference'
root = Path(__file__).resolve().parents[1]
fixture = root / 'tests/fixtures/legacy-resources.json'
cases = json.loads(fixture.read_text())
reference = args.reference_dll.resolve(strict=True)
rust = args.rust.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
source = fixture
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
actual_path = output / 'rust.json'
run(sandbox + ['dotnet', str(probe / 'bin/Debug/net9.0/probe.dll'), str(reference.parent), str(source), str(expected_path)], 'csharp')
run([str(rust), 'convert', str(source), str(actual_path)], 'rust')
expected = json.loads(expected_path.read_text())
actual = json.loads(actual_path.read_text())
def project(c):
    return {'Name': c['Name'], 'ResourceRegistry': [{'Url': url, **{k: meta.get(k, default) for k, default in {'ContentKey': None, 'MetadataHash': None, 'Files': {}, 'HandlerMetadata': {}, 'FileSize': 0}.items()}} for url, meta in c.get('ResourceRegistry', {}).items()]}
differences = [{'case': a['Name'], 'csharp': a, 'rust': project(b)} for a,b in zip(expected, actual) if a != project(b)]
receipt = {'reference_commit': 'd31ccbd3bfcddb4705917eafa6483715c166de3b', 'reference_dll_sha256': hashlib.sha256(reference.read_bytes()).hexdigest(), 'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(), 'cases': len(cases), 'differences': differences, 'scope': 'JSON loader legacy resource migration, ordered URLs, normalized hashes, nullable file flags and precedence. Not host downloads, cache behavior or all URI variants.'}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
print(json.dumps(receipt,indent=2))
assert len(expected) == len(actual) == len(cases)
# The pinned JSON loader discards modern Files maps. Retain exact evidence of
# this existing loader gap instead of dropping the compared field.
assert len(differences) == 1, 'Known modern Files difference changed; re-audit the fixture'
for difference in differences:
    assert difference['case'] == 'precedence'
    expected_case = json.loads(json.dumps(difference['csharp']))
    expected_case['ResourceRegistry'][0]['Files'] = {'keep.zip': False}
    assert expected_case == difference['rust']
