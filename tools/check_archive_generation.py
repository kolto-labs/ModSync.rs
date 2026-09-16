"""Capture raw local archive inference from the pinned C# service and compare Rust."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import zipfile
from xml.sax.saxutils import escape

PROBE = r'''using System.Runtime.Loader;
using System.Text.Json;
var reference = args[0];
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var path = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null;
};
Run(args.Skip(1).ToArray());
static void Run(string[] args) {
    var components = ModSync.Core.Services.ModComponentSerializationService.DeserializeModComponentFromString(File.ReadAllText(args[0]), "json");
    var output = new List<object>();
    var config = new ModSync.Core.MainConfig { sourcePath = new DirectoryInfo(args[1]) };
    using var input = JsonDocument.Parse(File.ReadAllText(args[0]));
    foreach (var (component, value) in components.Zip(input.RootElement.EnumerateArray())) {
        // Isolate inference from resource URL canonicalization in the JSON loader.
        var path = Path.Combine(args[1], value.GetProperty("ResourceRegistry").EnumerateObject().First().Name);
        // The JSON loader also rewrites authored Source separators. Assign the
        // original strings directly so both generation services receive identical data.
        if (value.TryGetProperty("Options", out var options)) {
            foreach (var (option, raw) in component.Options.Zip(options.EnumerateArray())) {
                if (raw.TryGetProperty("Instructions", out var instructions)) {
                    foreach (var (step, item) in option.Instructions.Zip(instructions.EnumerateArray())) {
                        if (item.TryGetProperty("Source", out var sources)) step.Source = sources.EnumerateArray().Select(s => s.GetString()).ToList();
                    }
                }
            }
        }
        var success = value.TryGetProperty("_LocalMatching", out var local) && local.GetBoolean()
            ? ModSync.Core.Services.AutoInstructionGenerator.TryGenerateInstructionsFromArchive(component)
            : ModSync.Core.Services.AutoInstructionGenerator.GenerateInstructions(component, path);
        object Step(ModSync.Core.Instruction i) => new { Action = i.Action.ToString(), Source = i.Action == ModSync.Core.Instruction.ActionType.Choose ? i.Source.Select(id => component.Options.First(o => o.Guid.ToString() == id).Name).ToList() : i.Source, i.Destination, i.Arguments, i.Overwrite, i.Dependencies, i.Restrictions };
        output.Add(new { component.Name, Success = success, component.InstallationMethod,
            Instructions = component.Instructions.Select(Step),
            Options = component.Options.Select(o => new { o.Name, o.Description, o.IsSelected, Instructions = o.Instructions.Select(Step) }) });
    }
    File.WriteAllText(args[2], JsonSerializer.Serialize(output, new JsonSerializerOptions { WriteIndented = true }));
}
'''

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll', type=Path, required=True)
parser.add_argument('--rust', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
assert shutil.which('bwrap'), 'bubblewrap is required to isolate the reference'
root = Path(__file__).resolve().parents[1]
fixture = root / 'tests/fixtures/archive-generation.json'
cases = json.loads(fixture.read_text())
reference = args.reference_dll.resolve(strict=True)
rust = args.rust.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
archives = output / 'archives'
archives.mkdir(exist_ok=True)
components = []
for index, case in enumerate(cases):
    filename = case['Name'] + '.' + case.get('Extension', 'zip')
    if 'Payload' in case:
        (archives / filename).write_bytes(case['Payload'].encode())
    else:
        with zipfile.ZipFile(archives / filename, 'w', zipfile.ZIP_DEFLATED) as archive:
            for name, content in case['Files'].items():
                archive.writestr(name, content)
    components.append({'Guid': f'00000000-0000-4000-8000-{index+1:012}', 'Name': case['Name'],
                       'ResourceRegistry': {filename: {}}, '_LocalMatching': case.get('LocalMatching', False), **case.get('Component', {})})
source = output / 'input.json'
source.write_text(json.dumps(components))
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
run(sandbox + ['dotnet', str(probe / 'bin/Debug/net9.0/probe.dll'), str(reference.parent), str(source), str(archives), str(expected_path)], 'csharp')
run([str(rust), 'convert', str(source), str(actual_path), '--auto-generate-local', '--source-path', str(archives)], 'rust')
expected = json.loads(expected_path.read_text())
actual = json.loads(actual_path.read_text())

def normalize(c):
    options = c.get('Options', [])
    def step(i):
        i = {k: v for k, v in i.items() if k not in ('Guid', 'Parent')}
        if i['Action'] == 'Choose':
            i['Source'] = [next(o['Name'] for o in options if o['Guid'] == ident) for ident in i['Source']]
        return i
    return {'Name': c['Name'], 'Success': bool(c['Instructions']),
            'InstallationMethod': c.get('InstallationMethod', ''),
            'Instructions': [step(i) for i in c['Instructions']],
            'Options': [{**{k: o[k] for k in ('Name','Description','IsSelected')}, 'Instructions': [step(i) for i in o['Instructions']]} for o in options]}

differences = []
unexpected = []
for case, a, b in zip(cases, expected, actual):
    normalized = normalize(b)
    if a == normalized:
        assert 'UnresolvedAdditionalInstruction' not in case, 'Previously observed difference changed; re-audit and remove its fixture annotation'
        continue
    annotated = copy.deepcopy(a)
    if 'UnresolvedAdditionalInstruction' in case:
        annotated['Instructions'].append(case['UnresolvedAdditionalInstruction'])
    known = annotated == normalized
    difference = {'case': a['Name'], 'csharp': a, 'rust': normalized, 'known_unresolved': known}
    differences.append(difference)
    if not known:
        unexpected.append(difference)
if len(expected) != len(actual):
    differences.append({'count': [len(expected), len(actual)]})
    unexpected.append(differences[-1])
receipt = {'reference_commit': 'd31ccbd3bfcddb4705917eafa6483715c166de3b',
           'reference_dll_sha256': hashlib.sha256(reference.read_bytes()).hexdigest(),
           'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(),
           'cases': len(cases), 'matching_cases': len(cases) - len(differences), 'differences': differences,
           'scope': 'Instruction-free component generation including authored-option merging, insertion-first local matching and EXE fallback. Original authored Source strings assigned directly after C# loading; Choose GUIDs normalized to option names. Not full JSON ingestion, all archive-matching cases, existing top-level instruction merging, real installer execution or full-build proof.'}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt, indent=2))
assert not unexpected
