"""Compare Rust draft instructions with the pinned C# draft service in a Linux sandbox."""
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
AssemblyLoadContext.Default.Resolving += (_, name) => {
    var path = Path.Combine(reference, name.Name + ".dll");
    return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null;
};
Run(args.Skip(1).ToArray());
static void Run(string[] args) {
    var components = ModSync.Core.Services.ModComponentSerializationService.DeserializeModComponentFromString(File.ReadAllText(args[0]), "json");
    // Assign prose directly: the legacy JSON loader drops some warning fields.
    // This check isolates DraftInstructionService from that separate format behavior.
    using var input = JsonDocument.Parse(File.ReadAllText(args[0]));
    foreach (var (component, value) in components.Zip(input.RootElement.EnumerateArray())) {
        component.Directions = value.TryGetProperty("Directions", out var directions) ? directions.GetString() ?? "" : "";
        component.DownloadInstructions = value.TryGetProperty("DownloadInstructions", out var download) ? download.GetString() ?? "" : "";
        component.InstallationWarning = value.TryGetProperty("InstallationWarning", out var warning) ? warning.GetString() ?? "" : "";
    }
    var results = ModSync.Core.Parsing.DraftInstructionService.GenerateDraftInstructions(components);
    var data = components.Select(c => new {
        c.Guid, c.Name,
        Instructions = c.Instructions.Select(i => new { Action = i.Action.ToString(), i.Source, i.Destination, i.Arguments, i.Overwrite, i.Dependencies, i.Restrictions }),
        Options = c.Options.Select(o => new { o.Name, o.Description, o.IsSelected }),
        c.InstallationWarning,
        Result = results.Where(r => r.Component.Guid == c.Guid).Select(r => new { r.DraftInstructionCount, r.UnparsedGaps, r.ConditionalDrafts }).FirstOrDefault()
    });
    File.WriteAllText(args[1], JsonSerializer.Serialize(data, new JsonSerializerOptions { WriteIndented = true }));
}
'''

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference-dll', type=Path, required=True)
parser.add_argument('--rust', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--input', type=Path, action='append')
args = parser.parse_args()
assert shutil.which('bwrap'), 'bubblewrap is required to isolate C# caches and network'
reference = args.reference_dll.resolve(strict=True)
rust = args.rust.resolve(strict=True)
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
inputs = args.input or [Path(__file__).resolve().parents[1] / 'tests/fixtures/drafts.json']
probe = output / 'probe'
probe.mkdir(exist_ok=True)
(probe / 'probe.csproj').write_text('<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net9.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><NuGetAudit>false</NuGetAudit></PropertyGroup><ItemGroup><Reference Include="ModSync.Core"><HintPath>' + escape(str(reference)) + '</HintPath><Private>false</Private></Reference></ItemGroup></Project>')
(probe / 'Program.cs').write_text(PROBE)

def file_name(destination):
    return ('.' in destination and destination.rsplit('.', 1)[1] != ''
            and not any(c in destination for c in '/\\:<>'))


def intentional_rename(x, y):
    """Rust drafts a Rename's new name as a bare file name, as C# RenameFile reads it.

    The pinned drafter gives every Rename a placeholder Destination (Rust refuses it;
    C# never renames as meant) and, for "rename a.ext to b.ext", the new name as a
    second Source. See docs/parity/drafted-rename-contract.md. When the engine refuses
    the new name (docs/parity/rename-file-name-contract.md), Rust keeps the placeholder
    too but still drafts one Source.
    """
    if len(x) != len(y):
        return False
    override = '<<kotorDirectory>>\\Override'
    for a, b in zip(x, y):
        if a == b:
            continue
        name = b.get('Destination') or ''
        if name == a.get('Destination') == override and len(a['Source']) == 2:
            name = a['Source'][1].replace('/', '\\').rsplit('\\', 1)[-1]
        elif not file_name(name):
            return False
        source = [s for s in a['Source'] if s.replace('/', '\\').rsplit('\\', 1)[-1].lower() != name.lower()]
        if not (a['Action'] == b['Action'] == 'Rename'
                and a.get('Destination') == override and b['Source'] == source
                and {k: v for k, v in a.items() if k not in ('Source', 'Destination')}
                == {k: v for k, v in b.items() if k not in ('Source', 'Destination')}):
            return False
    return True


def run(command, label):
    result = subprocess.run(command, cwd=output, capture_output=True, timeout=300)
    (output / (label + '.log')).write_bytes(result.stdout + result.stderr)
    assert result.returncode == 0, f'{label} failed; see its log in {output}'

run(['dotnet', 'build', str(probe / 'probe.csproj'), '-v', 'quiet'], 'build-probe')
sandbox = ['bwrap', '--ro-bind', '/', '/', '--dev', '/dev', '--tmpfs', '/tmp',
           '--tmpfs', str(Path.home() / '.local/share'), '--tmpfs', str(Path.home() / '.config'),
           '--ro-bind', str(reference.parent), str(reference.parent),
           '--bind', str(output), str(output), '--unshare-net', '--die-with-parent']
checks = []
for index, source in enumerate(inputs):
    source = source.resolve(strict=True)
    cs = output / f'{index}-csharp.json'
    rs = output / f'{index}-rust.json'
    staged = output / f'{index}-input.json'  # the sandbox hides /tmp but binds the output
    shutil.copyfile(source, staged)
    run(sandbox + ['dotnet', str(probe / 'bin/Debug/net9.0/probe.dll'), str(reference.parent), str(staged), str(cs)], f'{index}-csharp')
    run([str(rust), 'convert', str(source), str(rs), '--parse-directions', '--format', 'json'], f'{index}-rust')
    expected = json.loads(cs.read_text())
    actual = json.loads(rs.read_text())
    differences, known = [], []
    if len(expected) != len(actual):
        differences.append({'field': 'component_count', 'csharp': len(expected), 'rust': len(actual)})
    for a, b in zip(expected, actual):
        for key in ['Guid', 'Name', 'Instructions', 'Options', 'InstallationWarning']:
            x, y = a.get(key, ''), b.get(key, '')
            if key == 'Instructions':
                y = [{k: v for k, v in i.items() if k not in ('Guid', 'Parent')} for i in y]
            if key == 'Options':
                y = [{k: o[k] for k in ('Name', 'Description', 'IsSelected')} for o in y]
            if key == 'InstallationWarning':
                x, y = x.replace('\r\n', '\n'), y.replace('\r\n', '\n')
            if x != y:
                entry = {'component': a['Name'], 'field': key, 'csharp': x, 'rust': y}
                (known if key == 'Instructions' and intentional_rename(x, y) else differences).append(entry)
        if a['Result'] is not None:
            result = b.get('DraftReview', {}).get('Result', {})
            for key, default in [('DraftInstructionCount', 0), ('UnparsedGaps', []), ('ConditionalDrafts', [])]:
                x, y = a['Result'][key], result.get(key, default)
                if x != y:
                    differences.append({'component': a['Name'], 'field': key, 'csharp': x, 'rust': y})
    checks.append({'input': str(source), 'input_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                   'components': len(expected), 'draft_instructions': sum(len(c['Instructions']) for c in expected), 'differences': differences,
                   'intentional_rename_differences': known})
    print(f'{source.name}: {len(expected)} components; {len(differences)} differences; '
          f'{len(known)} intentional Rename differences', flush=True)
receipt = {'reference_dll_sha256': hashlib.sha256(reference.read_bytes()).hexdigest(), 'checks': checks,
           'scope': 'Prose fields assigned directly to isolate DraftInstructionService from legacy JSON-loader omissions. Raw instructions, option fields excluding generated IDs, review warnings, unparsed gaps and conditional notes. A drafted Rename whose Rust Destination is the bare new file name (and whose Sources leave out that name), or the same placeholder with the second Source (the refused new name) left out, is listed under intentional_rename_differences, not differences. Not final install correctness or full guide ingestion.'}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
assert not any(c['differences'] for c in checks), f'Draft differences recorded in {output / "receipt.json"}'
