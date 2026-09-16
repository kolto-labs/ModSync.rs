"""Inspect modern resource field preservation across C# and Rust format loaders. This is a gap report, not a passing parity gate."""
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
var components = ModSync.Core.Services.ModComponentSerializationService.DeserializeModComponentFromString(File.ReadAllText(args[0]), args[2]);
File.WriteAllText(args[1], JsonSerializer.Serialize(components.Select(c => new {c.Name, ResourceRegistry = c.ResourceRegistry.Select(r => new { Url = r.Key, r.Value.ContentKey, r.Value.MetadataHash, r.Value.Files, r.Value.HandlerMetadata, r.Value.FileSize, r.Value.FirstSeen, r.Value.LastVerified })}), new JsonSerializerOptions {WriteIndented=true}));
}
'''

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll', type=Path, required=True)
parser.add_argument('--rust', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
assert shutil.which('bwrap'), 'bubblewrap is required to isolate the reference'
root = Path(__file__).resolve().parents[1]
fixture = root / 'tests/fixtures/modern-resources.json'
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
results = []
for format in ['json', 'toml', 'yaml', 'xml']:
    source_path = output / ('input.' + format)
    run([str(rust), 'convert', str(source), str(source_path)], 'write-' + format)
    expected_path = output / ('csharp-' + format + '.json')
    actual_path = output / ('rust-' + format + '.json')
    run(sandbox + ['dotnet', str(probe / 'bin/Debug/net9.0/probe.dll'), str(reference.parent), str(source_path), str(expected_path), format], 'csharp-' + format)
    run([str(rust), 'convert', str(source_path), str(actual_path)], 'rust-' + format)
    expected = json.loads(expected_path.read_text())
    actual = json.loads(actual_path.read_text())
    results.append({'format': format, 'input_sha256': hashlib.sha256(source_path.read_bytes()).hexdigest(), 'csharp': expected, 'rust': actual})
receipt = {'reference_commit': 'd31ccbd3bfcddb4705917eafa6483715c166de3b', 'reference_dll_sha256': hashlib.sha256(reference.read_bytes()).hexdigest(), 'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(), 'results': results, 'scope': 'Inspection only: raw C# resource fields and Rust loaded model for the same per-format input. Inputs rendered by Rust from a modern resource fixture. No field projection or equivalence assertion; recorded differences remain open.'}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
print(str(output / 'receipt.json'))
