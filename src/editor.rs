use crate::{Component, Instruction};
use anyhow::{Result, bail};

pub fn move_component(components: &mut [Component], guid: &str, offset: isize) -> Result<()> {
    let index = components
        .iter()
        .position(|component| component.guid.eq_ignore_ascii_case(guid))
        .ok_or_else(|| anyhow::anyhow!("Unknown component GUID: {guid}"))?;
    let destination = index
        .checked_add_signed(offset)
        .filter(|index| *index < components.len())
        .ok_or_else(|| anyhow::anyhow!("Component move is out of range"))?;
    components.swap(index, destination);
    Ok(())
}

pub fn remove_component(components: &mut Vec<Component>, guid: &str) -> Result<Component> {
    let index = components
        .iter()
        .position(|component| component.guid.eq_ignore_ascii_case(guid))
        .ok_or_else(|| anyhow::anyhow!("Unknown component GUID: {guid}"))?;
    if components.iter().any(|component| {
        component
            .dependencies
            .iter()
            .any(|dependency| dependency.eq_ignore_ascii_case(guid))
    }) {
        bail!("Cannot remove a component required by another component");
    }
    Ok(components.remove(index))
}

pub fn move_instruction(component: &mut Component, index: usize, offset: isize) -> Result<()> {
    let destination = index
        .checked_add_signed(offset)
        .filter(|index| *index < component.instructions.len())
        .ok_or_else(|| anyhow::anyhow!("Instruction move is out of range"))?;
    component.instructions.swap(index, destination);
    Ok(())
}

pub fn add_instruction(component: &mut Component, index: usize) -> Result<()> {
    if index > component.instructions.len() {
        bail!("Instruction insert is out of range");
    }
    component.instructions.insert(
        index,
        Instruction {
            guid: String::new(),
            parent: String::new(),
            action: "Unset".into(),
            source: vec![],
            destination: String::new(),
            arguments: String::new(),
            overwrite: true,
            dependencies: vec![],
            restrictions: vec![],
        },
    );
    Ok(())
}
