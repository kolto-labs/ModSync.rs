# Example build

Read before installing.

## Mod List

### Display title
**Name:** [Base textures](https://example.com/base.zip)
**Author:** Author One
**Description:** First line.

Second line.
**Category & Tier:** Textures & Models / Essential
**Non-English Functionality:** All
:::note
Installation Instructions
: Copy the chosen textures.
:::
<!--<<ModSync>>
Guid: 11111111-1111-1111-1111-111111111111
IsSelected: true
Instructions:
- Action: Copy
  Source: '<<modDirectory>>/sample.tga'
  Destination: '<<kotorDirectory>>/Override'
-->
___

### Extra textures
Name: Extra textures
Author: Author Two
Description: Additional files.
Category & Tier: Textures / Optional
Installation Instructions:
Copy the second texture.
<!--<<ModSync>>
[[thisMod]]
Guid = "22222222-2222-2222-2222-222222222222"
IsSelected = true
Dependencies = ["11111111-1111-1111-1111-111111111111"]
[[thisMod.Instructions]]
Action = "Copy"
Source = ["<<modDirectory>>/extra.tga"]
Destination = "<<kotorDirectory>>/Override"
-->

## After installation

### This is not a mod
Do not import this heading.
