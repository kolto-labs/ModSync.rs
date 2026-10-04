"""Capture raw local archive inference from the pinned C# service and compare Rust."""
import argparse
import copy
import hashlib
import json
import os
import re
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
        // The JSON loader rewrites authored Source separators. The "raw" capture
        // assigns the original strings, as tests/archive_generation.rs builds
        // components without the Rust loader. The "loader" capture keeps C#'s
        // loaded Sources, as Rust's convert loader normalizes them the same way.
        if (args[3] == "raw") {
            if (value.TryGetProperty("Instructions", out var topInstructions)) {
                foreach (var (step, item) in component.Instructions.Zip(topInstructions.EnumerateArray())) {
                    if (item.TryGetProperty("Source", out var sources)) step.Source = sources.EnumerateArray().Select(s => s.GetString()).ToList();
                }
            }
            if (value.TryGetProperty("Options", out var options)) {
                foreach (var (option, raw) in component.Options.Zip(options.EnumerateArray())) {
                    if (raw.TryGetProperty("Instructions", out var instructions)) {
                        foreach (var (step, item) in option.Instructions.Zip(instructions.EnumerateArray())) {
                            if (item.TryGetProperty("Source", out var sources)) step.Source = sources.EnumerateArray().Select(s => s.GetString()).ToList();
                        }
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
    File.WriteAllText(args[4], System.Runtime.InteropServices.RuntimeInformation.FrameworkDescription);
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
# The sandbox mounts a tmpfs over /tmp and then binds the reference directory.
# Binding /tmp itself (or an ancestor) would re-expose the host /tmp.
assert reference.parent not in (Path('/tmp'), *Path('/tmp').parents), 'place the reference DLL in its own directory, not directly in /tmp'
rust = args.rust.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
archives = output / 'archives'
archives.mkdir(exist_ok=True)
raw_archives = output / 'archives-raw'
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
# The reference may delete an archive it finds corrupt, so the raw capture
# gets its own copy of the inputs.
shutil.rmtree(raw_archives, ignore_errors=True)
shutil.copytree(archives, raw_archives)
source = output / 'input.json'
source.write_text(json.dumps(components))
probe = output / 'probe'
probe.mkdir(exist_ok=True)
(probe / 'probe.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net9.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="ModSync.Core"><HintPath>' + escape(str(reference)) + '</HintPath><Private>false</Private></Reference></ItemGroup></Project>')
(probe / 'Program.cs').write_text(PROBE)

# The probe targets net9.0; allow a newer installed runtime to load it.
environment = {**os.environ, 'DOTNET_ROLL_FORWARD': os.environ.get('DOTNET_ROLL_FORWARD', 'Major')}

def run(command, label):
    result = subprocess.run(command, cwd=output, capture_output=True, timeout=300, env=environment)
    (output / (label + '.log')).write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, f'{label} failed; see {output}/{label}.log'

# The reference falls back to the 7-Zip CLI when SharpCompress cannot list an
# archive; without one that reads ZIP, a ZIP renamed to .exe becomes Execute
# instead of Extract. Use the first PATH name the reference tries.
seven_zip = next((shutil.which(n) for n in ('7z', '7za', '7zr') if shutil.which(n)), None)
assert seven_zip, 'the 7-Zip CLI is required by the reference fallback'
check_zip = output / '7zip-check.zip'
with zipfile.ZipFile(check_zip, 'w') as archive:
    archive.writestr('check.txt', 'check')
listing = subprocess.run([seven_zip, 'l', str(check_zip)], capture_output=True, text=True, timeout=60)
assert listing.returncode == 0 and 'check.txt' in listing.stdout, f'{seven_zip} cannot list a ZIP archive'
seven_zip_version = next((line.strip() for line in listing.stdout.splitlines() if '7-Zip' in line), '')

run(['dotnet', 'build', str(probe / 'probe.csproj'), '-v', 'quiet'], 'build-probe')
sandbox = ['bwrap', '--ro-bind', '/', '/', '--dev', '/dev', '--tmpfs', '/tmp',
           '--tmpfs', str(Path.home() / '.local/share'), '--tmpfs', str(Path.home() / '.config'),
           '--ro-bind', str(reference.parent), str(reference.parent),
           '--bind', str(output), str(output), '--unshare-net', '--die-with-parent']
expected_path = output / 'csharp.json'
raw_path = output / 'csharp-raw.json'
runtime_path = output / 'runtime.txt'
actual_path = output / 'rust.json'
probe_dll = str(probe / 'bin/Debug/net9.0/probe.dll')
run(sandbox + ['dotnet', probe_dll, str(reference.parent), str(source), str(raw_archives), str(raw_path), 'raw', str(runtime_path)], 'csharp-raw')
run(sandbox + ['dotnet', probe_dll, str(reference.parent), str(source), str(archives), str(expected_path), 'loader', str(runtime_path)], 'csharp')
# tests/archive_generation.rs compares this committed capture of the raw mode.
integration_fixture = root / 'tests/fixtures/csharp/archive-generation.json'
integration_matches = raw_path.read_bytes() == integration_fixture.read_bytes()
run([str(rust), 'convert', str(source), str(actual_path), '--auto-generate-local', '--source-path', str(archives), '-f', 'json'], 'rust')
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
    if case.get('DirectApi'):
        # convert --auto-generate-local intentionally skips authored instructions.
        # The captured raw objects for this public API are compared by Rust tests.
        continue
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
reference_bytes = reference.read_bytes()
# AssemblyInformationalVersion is stored as UTF-16 in the user-string heap.
informational = sorted({m for text in (reference_bytes.decode('utf-16-le', 'ignore'), reference_bytes[1:].decode('utf-16-le', 'ignore'))
                        for m in re.findall(r'\d+\.\d+\.\d+[\w.-]*\+[0-9a-f]{40}', text)})
receipt = {'reference_commit': 'd31ccbd3bfcddb4705917eafa6483715c166de3b',
           'reference_dll_sha256': hashlib.sha256(reference_bytes).hexdigest(),
           'reference_informational_version': informational,
           'reference_dll_note': 'Any local build of d31ccbd3; bytes, and so this hash, vary with build path and SDK. Sibling receipts record another build (8f3b1e3f...).',
           'reference_runtime': runtime_path.read_text(),
           'dotnet_roll_forward': environment['DOTNET_ROLL_FORWARD'],
           'seven_zip': {'path': seven_zip, 'version': seven_zip_version},
           'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(),
           'cases': len(cases),
           'direct_api_fixture_cases': [c['Name'] for c in cases if c.get('DirectApi')],
           'matching_cli_cases': sum(not c.get('DirectApi') for c in cases) - len(differences), 'differences': differences,
           'integration_capture': {'fixture': str(integration_fixture.relative_to(root)),
                                   'sha256': hashlib.sha256(raw_path.read_bytes()).hexdigest(),
                                   'matches_committed_fixture': integration_matches},
           'scope': 'Two C# captures, one per mode. CLI comparison (csharp.json): instruction-free generation on Sources as each JSON loader stores them; Rust convert normalizes authored Sources like C# since 62b6f70, so the probe keeps C# loaded Sources. Integration-test capture (csharp-raw.json, all cases including direct-API ones): original authored Source strings assigned after C# loading, because tests/archive_generation.rs builds components without the Rust loader; it must equal the committed tests/fixtures/csharp/archive-generation.json. Choose GUIDs normalized to option names. Not full JSON ingestion, all matching cases, real installer execution or full-build proof.'}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt, indent=2))
assert not unexpected
assert integration_matches, f'{raw_path} differs from {integration_fixture}'
