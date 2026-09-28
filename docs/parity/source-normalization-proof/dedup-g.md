# Probe G

```toml
[[thisMod]]
Guid = "00000000-0000-4000-8000-000000000800"
Name = "Probe G"
IsSelected = true

[[thisMod.Instructions]]
Guid = "88888888-0000-4000-8000-000000000800"
Action = "Extract"
Source = ["<<modDirectory>>\\Probe_G*.zip"]

[[thisMod.Instructions]]
Guid = "98888888-0000-4000-8000-000000000800"
Action = "Choose"
Source = ["00000000-0000-4000-8000-000000000801", "00000000-0000-4000-8000-000000000802"]

[[thisMod.Options]]
Guid = "00000000-0000-4000-8000-000000000801"
Name = "G1"
IsSelected = false
[[thisMod.Options.Instructions]]
Guid = "77777777-0000-4000-8000-000000000801"
Action = "Move"
Parent = "00000000-0000-4000-8000-000000000801"
Source = ["<<modDirectory>>\\x\\*.tga"]
Destination = "<<kotorDirectory>>\\Override"

[[thisMod.Options]]
Guid = "00000000-0000-4000-8000-000000000802"
Name = "G2"
IsSelected = false
[[thisMod.Options.Instructions]]
Guid = "77777777-0000-4000-8000-000000000802"
Action = "Move"
Parent = "00000000-0000-4000-8000-000000000802"
Source = ["<<modDirectory>>\\x\\*.tga"]
Destination = "<<kotorDirectory>>\\Override"

```
