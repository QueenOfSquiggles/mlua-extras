use std::{borrow::Cow, path::Path, slice::Iter};

use crate::typed::{function::Return, Param, Type, TypedModuleBuilder};

use super::{Definition, Definitions};

/// Generates a lua definition file for each [`Definition`][`crate::typed::generator::Definition`]
///
/// Each file will start with `--- @meta` and contain types inside of doc comment to be used with
/// [LuaLsp](https://github.com/LuaLS/lua-language-server). If there are expose values those are
/// written as `{name} = nil` with a `--- @type {type}` doc comment above to mark it's value.
///
/// # Example Output
///
/// ```lua
/// --- @meta
///
/// --- @class Example
/// --- Name of the example
/// --- @field name string
/// --- Run the example returning it's success state
/// --- @field run fun(): bool
///
/// --- Global example
/// --- @type Example
/// example = nil
/// ```
pub struct DefinitionFileGenerator<'def> {
    /// Extendion of each definition file: Default [`.d.lua`]
    ///
    /// **IMPORTANT** Must start with a dot
    extension: String,
    definitions: Definitions<'def>,
}

impl<'def> Default for DefinitionFileGenerator<'def> {
    fn default() -> Self {
        Self {
            extension: ".d.lua".into(),
            definitions: Definitions::default(),
        }
    }
}

impl<'def> DefinitionFileGenerator<'def> {
    /// Create a new generator given a collection of definitions
    pub fn new(definitions: Definitions<'def>) -> Self {
        Self {
            definitions,
            ..Default::default()
        }
    }

    /// Set the extension that each file will end with
    pub fn ext(mut self, ext: impl AsRef<str>) -> Self {
        self.extension = ext.as_ref().to_string();
        self
    }

    pub fn iter(&self) -> DefinitionFileIter<'_> {
        DefinitionFileIter {
            extension: self.extension.clone(),
            definitions: self.definitions.iter(),
        }
    }
}

pub struct DefinitionFileIter<'def> {
    extension: String,
    definitions: Iter<'def, (Cow<'def, str>, Definition<'def>)>,
}

impl<'def> Iterator for DefinitionFileIter<'def> {
    type Item = (String, DefinitionWriter<'def>);

    fn next(&mut self) -> Option<Self::Item> {
        self.definitions.next().map(|v| {
            (
                format!("{}{}", v.0, self.extension),
                DefinitionWriter { definition: &v.1 },
            )
        })
    }
}

pub struct DefinitionWriter<'def> {
    definition: &'def Definition<'def>,
}

impl DefinitionWriter<'_> {
    /// Write the full definition group to a specified file
    pub fn write_file<P: AsRef<Path>>(&self, path: P) -> mlua::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        self.write(&mut file)
    }

    /// PERF: Check if there is a good api for adding color when printing to stdout, stderr, etc
    ///
    /// Write the full definition group to the specified `io`
    pub fn write<W: std::io::Write>(&self, mut buffer: W) -> mlua::Result<()> {
        writeln!(buffer, "--- @meta\n")?;

        for definition in self.definition.iter() {
            match &definition.ty {
                Type::Value(ty) => {
                    if let Some(docs) = Self::accumulate_docs(&[definition.doc.as_deref()]) {
                        writeln!(buffer, "{}", docs.join("\n"))?;
                    }

                    writeln!(buffer, "--- @type {}", Self::type_signature(ty)?)?;
                    writeln!(buffer, "{} = nil", definition.name)?;
                }
                Type::Class(type_data) => {
                    if let Some(docs) =
                        Self::accumulate_docs(&[definition.doc.as_deref(), type_data.type_doc.as_deref()])
                    {
                        writeln!(buffer, "{}", docs.join("\n"))?;
                    }
                    writeln!(buffer, "--- @class {}", definition.name)?;

                    for (name, field) in type_data.static_fields.iter().filter(|(k, _)| !needs_escape(k.as_ref())) {
                        if let Some(docs) = Self::accumulate_docs(&[field.doc.as_deref()]) {
                            writeln!(buffer, "{}", docs.join("\n"))?;
                        }
                        writeln!(
                            buffer,
                            "--- @field {name} {}",
                            Self::type_signature(&field.ty)?
                        )?;
                    }

                    for (name, field) in type_data.fields.iter().filter(|(k, _)| !needs_escape(k.as_ref())) {
                        if let Some(docs) = Self::accumulate_docs(&[field.doc.as_deref()]) {
                            writeln!(buffer, "{}", docs.join("\n"))?;
                        }
                        writeln!(
                            buffer,
                            "--- @field {name} {}",
                            Self::type_signature(&field.ty)?
                        )?;
                    }

                    writeln!(buffer, "local _Class_{} = {{", definition.name)?;
                    for (name, field) in type_data.static_fields.iter().filter(|(k, _)| needs_escape(k.as_ref())) {
                        if let Some(docs) = Self::accumulate_docs(&[field.doc.as_deref()]) {
                            writeln!(buffer, "  {}", docs.join("\n  "))?;
                        }
                        writeln!(buffer, "  --- @type {}", Self::type_signature(&field.ty)?)?;
                        writeln!(buffer, "  [\"{name}\"] = nil,")?;
                    }

                    for (name, field) in type_data.fields.iter().filter(|(k, _)| needs_escape(k.as_ref())) {
                        if let Some(docs) = Self::accumulate_docs(&[field.doc.as_deref()]) {
                            writeln!(buffer, "  {}", docs.join("\n  "))?;
                        }
                        writeln!(buffer, "  --- @type {}", Self::type_signature(&field.ty)?)?;
                        writeln!(buffer, "  [\"{name}\"] = nil,")?;
                    }

                    for (name, func) in type_data.functions.iter() {
                        if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                            writeln!(buffer, "  {}", docs.join("\n  "))?;
                        }
                        writeln!(
                            buffer,
                            "  {},",
                            Self::function_signature(
                                escape_key(name.as_ref()),
                                &func.params,
                                &func.returns,
                                true
                            )?
                            .join("\n  ")
                        )?;
                    }

                    for (name, func) in type_data.methods.iter() {
                        if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                            writeln!(buffer, "  {}", docs.join("\n  "))?;
                        }
                        writeln!(
                            buffer,
                            "  {},",
                            Self::method_signature(
                                escape_key(name.as_ref()),
                                definition.name.to_string(),
                                &func.params,
                                &func.returns,
                                true
                            )?
                            .join("\n  ")
                        )?;
                    }

                    if !type_data.is_meta_empty() {
                        if !type_data.meta_fields.is_empty()
                            || !type_data.meta_functions.is_empty()
                            || !type_data.meta_methods.is_empty()
                        {
                            writeln!(buffer, "  __metatable = {{")?;
                            for (name, field) in type_data.meta_fields.iter() {
                                if let Some(docs) = Self::accumulate_docs(&[field.doc.as_deref()]) {
                                    writeln!(buffer, "    {}", docs.join("\n    "))?;
                                }
                                writeln!(buffer, "--- @type {}", Self::type_signature(&field.ty)?)?;
                                writeln!(buffer, "{name} = nil,")?;
                            }

                            for (name, func) in type_data.meta_functions.iter() {
                                if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                                    writeln!(buffer, "    {}", docs.join("\n    "))?;
                                }
                                writeln!(
                                    buffer,
                                    "    {},",
                                    Self::function_signature(
                                        escape_key(name.as_ref()),
                                        &func.params,
                                        &func.returns,
                                        true
                                    )?
                                    .join("\n    ")
                                )?;
                            }

                            for (name, func) in type_data.meta_methods.iter() {
                                if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                                    writeln!(buffer, "    {}", docs.join("\n    "))?;
                                }
                                writeln!(
                                    buffer,
                                    "    {},",
                                    Self::method_signature(
                                        escape_key(name.as_ref()),
                                        definition.name.to_string(),
                                        &func.params,
                                        &func.returns,
                                        true
                                    )?
                                    .join("\n    ")
                                )?;
                            }
                            writeln!(buffer, "  }}")?;
                        }

                    }
                    writeln!(buffer, "}}")?;
                }
                Type::Enum(name, types) => {
                    if let Some(docs) = Self::accumulate_docs(&[definition.doc.as_deref()]) {
                        writeln!(buffer, "{}", docs.join("\n"))?;
                    }
                    writeln!(
                        buffer,
                        "--- @alias {name} {}",
                        types
                            .iter()
                            .map(Self::type_signature)
                            .collect::<mlua::Result<Vec<_>>>()?
                            .join("\n---  | ")
                    )?;
                }
                Type::Alias(ty) => {
                    if let Some(docs) = Self::accumulate_docs(&[definition.doc.as_deref()]) {
                        writeln!(buffer, "{}", docs.join("\n"))?;
                    }
                    writeln!(
                        buffer,
                        "--- @alias {} {}",
                        definition.name,
                        Self::type_signature(ty)?
                    )?;
                }
                Type::Function { params, returns } => {
                    if let Some(docs) = Self::accumulate_docs(&[definition.doc.as_deref()]) {
                        writeln!(buffer, "{}", docs.join("\n"))?;
                    }
                    writeln!(
                        buffer,
                        "{}",
                        Self::function_signature(
                            escape_key(definition.name.as_ref()),
                            params,
                            returns,
                            false
                        )?
                        .join("\n")
                    )?;
                }
                Type::Module(module) => {
                    if let Some(docs) =
                        Self::accumulate_docs(&[definition.doc.as_deref(), module.doc.as_deref()])
                    {
                        writeln!(buffer, "{}", docs.join("\n"))?;
                    }

                    write!(buffer, "{} = ", definition.name)?;
                    let mut path = Vec::new();
                    Self::write_module(&mut buffer, module, &mut path)?;
                    writeln!(buffer)?;
                },
                other => {
                    return Err(mlua::Error::runtime(format!(
                        "invalid root level type: {}",
                        other.as_ref()
                    )))
                }
            }
            writeln!(buffer)?;
        }

        Ok(())
    }

    fn function_signature<S: std::fmt::Display>(
        name: S,
        params: &[Param],
        returns: &[Return],
        assign: bool,
    ) -> mlua::Result<Vec<String>> {
        let mut result = Vec::new();

        for (i, param) in params.iter().enumerate() {
            let doc = param.doc.as_deref().unwrap_or_default();
            result.push(match param.name.as_deref() {
                Some(name) => format!("--- @param {name} {} {doc}", Self::type_signature(&param.ty)?),
                None => format!("--- @param param{i} {} {doc}", Self::type_signature(&param.ty)?),
            });
        }

        for ret in returns.iter() {
            let doc = ret.doc.as_deref().unwrap_or_default();
            result.push(format!("--- @return {} {doc}", Self::type_signature(&ret.ty)?));
        }

        result.push(format!(
            "{}function{}({}) end",
            if assign {
                format!("{name} = ")
            } else {
                String::new()
            },
            if !assign {
                format!(" {name}")
            } else {
                String::new()
            },
            params
                .iter()
                .enumerate()
                .map(|(i, v)| v
                    .name
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or(format!("param{i}")))
                .collect::<Vec<_>>()
                .join(", "),
        ));
        Ok(result)
    }

    fn method_signature<S: std::fmt::Display>(
        name: S,
        class: String,
        params: &[Param],
        returns: &[Return],
        assign: bool,
    ) -> mlua::Result<Vec<String>> {
        let mut result = Vec::from([format!("--- @param self {class}")]);
        for (i, param) in params.iter().enumerate() {
            let doc = param.doc.as_deref().unwrap_or_default();
            result.push(match param.name.as_deref() {
                Some(name) => format!("--- @param {name} {} {doc}", Self::type_signature(&param.ty)?),
                None => format!("--- @param param{i} {} {doc}", Self::type_signature(&param.ty)?),
            });
        }

        for ret in returns.iter() {
            let doc = ret.doc.as_deref().unwrap_or_default();
            result.push(format!("--- @return {} {doc}", Self::type_signature(&ret.ty)?));
        }

        result.push(format!(
            "{}function{}({}{}) end",
            if assign {
                format!("{name} = ")
            } else {
                String::new()
            },
            if !assign {
                format!(" {name}")
            } else {
                String::new()
            },
            if params.is_empty() { "self" } else { "self, " },
            params
                .iter()
                .enumerate()
                .map(|(i, v)| v
                    .name
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or(format!("param{i}")))
                .collect::<Vec<_>>()
                .join(", "),
        ));
        Ok(result)
    }

    fn type_signature(ty: &Type) -> mlua::Result<String> {
        Ok(match ty {
            Type::Enum(name, _) => name.to_string(),
            Type::Single(value) => value.to_string(),
            Type::Tuple(types) => {
                format!(
                    "{{ {} }}",
                    types
                        .iter()
                        .enumerate()
                        .map(|(i, t)| Ok(format!("[{}]: {}", i + 1, Self::type_signature(t)?)))
                        .collect::<mlua::Result<Vec<_>>>()?
                        .join(", ")
                )
            }
            Type::Variadic(ty) => {
                format!("...{}", Self::type_signature(ty)?)
            }
            Type::Array(ty) => {
                format!("{{ [integer]: {} }}", Self::type_signature(ty)?)
            }
            Type::Map(key, value) => {
                format!(
                    "{{ [{}]: {} }}",
                    Self::type_signature(key)?,
                    Self::type_signature(value)?
                )
            }
            Type::Function { params, returns } => {
                format!(
                    "fun({}){}",
                    params
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            v.name
                                .as_ref()
                                .map(|v| v.to_string())
                                .unwrap_or(format!("param{i}"))
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    if returns.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ": {}",
                            returns
                                .iter()
                                .map(|v| Self::type_signature(&v.ty))
                                .collect::<mlua::Result<Vec<_>>>()?
                                .join(", ")
                        )
                    }
                )
            }
            Type::Union(types) => types
                .iter()
                .map(Self::type_signature)
                .collect::<mlua::Result<Vec<_>>>()?
                .join(" | "),
            Type::Table(entries) => {
                format!(
                    "{{ {} }}",
                    entries
                        .iter()
                        .map(|(k, v)| { Ok(format!("{k}: {}", Self::type_signature(v)?)) })
                        .collect::<mlua::Result<Vec<_>>>()?
                        .join(", ")
                )
            }
            other => {
                return Err(mlua::Error::runtime(format!(
                    "type cannot be a type signature: {}",
                    other.as_ref()
                )))
            }
        })
    }

    fn accumulate_docs(docs: &[Option<&str>]) -> Option<Vec<String>> {
        let docs = docs.iter().filter_map(|v| *v).collect::<Vec<_>>();
        (!docs.is_empty()).then_some({
            docs.iter()
                .flat_map(|v| v.split('\n').map(|v| format!("--- {v}")))
                .collect::<Vec<_>>()
        })
    }

    fn write_module<B: std::io::Write>(buffer: &mut B, module: &TypedModuleBuilder, path: &mut Vec<String>) -> mlua::Result<()> {
        let indent = path.len()*2;
        let current_offset = (0..indent).map(|_| ' ').collect::<String>();
        let single_offset = (0..indent+2).map(|_| ' ').collect::<String>();

        if module.is_empty() {
            write!(buffer, "{{}}")?;
            return Ok(())
        } else {
            writeln!(buffer, "{{")?;
        }

        for (name, field) in module.fields.iter() {
            if let Some(docs) = Self::accumulate_docs(&[field.doc.as_deref()]) {
                writeln!(buffer, "{single_offset}{}", docs.join(format!("\n{single_offset}").as_str()))?;
            }

            match &field.ty {
                &Type::Module(ref module) => {
                    write!(buffer, "{single_offset}{name} = ")?;
                    path.push(name.to_string());
                    Self::write_module(buffer, module, path)?;
                    path.pop();
                    writeln!(buffer, ",")?;
                },
                other => {
                    writeln!(buffer, "{single_offset}--- @type {}", Self::type_signature(other)?)?;
                    writeln!(buffer, "{single_offset}{name} = nil,")?
                },
            }
        }

        for (name, nested) in module.nested_modules.iter() {
            if let Some(docs) = Self::accumulate_docs(&[nested.doc.as_deref()]) {
                writeln!(buffer, "{single_offset}{}", docs.join(format!("\n{single_offset}").as_str()))?;
            }

            write!(buffer, "{single_offset}{name} = ")?;
            path.push(name.to_string());
            Self::write_module(buffer, nested, path)?;
            path.pop();
            writeln!(buffer, ",")?;
        }

        for (name, func) in module.functions.iter() {
            if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                writeln!(buffer, "{single_offset}{}", docs.join(format!("\n{single_offset}").as_str()))?;
            }

            writeln!(buffer, "{single_offset}{},", Self::function_signature(name, &func.params, &func.returns, true)?.join(format!("\n{single_offset}").as_str()))?;
        }

        for (name, func) in module.methods.iter() {
            if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                writeln!(buffer, "{single_offset}{}", docs.join(format!("\n{single_offset}").as_str()))?;
            }

            writeln!(buffer, "{single_offset}{},", Self::method_signature(name, "table".into(), &func.params, &func.returns, true)?.join(format!("\n{single_offset}").as_str()))?;
        }

        if !module.is_meta_empty() {
            writeln!(buffer, "{single_offset}__metatable = {{")?;

            let double_offset = (0..indent+4).map(|_| ' ').collect::<String>();

            for (name, field) in module.meta_fields.iter() {
                if let Some(docs) = Self::accumulate_docs(&[field.doc.as_deref()]) {
                    writeln!(buffer, "{double_offset}{}", docs.join(format!("\n{single_offset}").as_str()))?;
                }

                match &field.ty {
                    &Type::Module(ref module) => {
                        write!(buffer, "{double_offset}{name} = ")?;
                        path.push(name.to_string());
                        Self::write_module(buffer, module, path)?;
                        path.pop();
                        writeln!(buffer, ",")?;
                    },
                    other => {
                        writeln!(buffer, "{double_offset}--- @type {}", Self::type_signature(other)?)?;
                        writeln!(buffer, "{double_offset}{name} = nil,")?
                    },
                }
            }

            for (name, func) in module.meta_functions.iter() {
                if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                    writeln!(buffer, "{double_offset}{}", docs.join(format!("\n{double_offset}").as_str()))?;
                }

                writeln!(buffer, "{double_offset}{},", Self::function_signature(name, &func.params, &func.returns, true)?.join(format!("\n{double_offset}").as_str()))?;
            }

            for (name, func) in module.meta_methods.iter() {
                if let Some(docs) = Self::accumulate_docs(&[func.doc.as_deref()]) {
                    writeln!(buffer, "{double_offset}{}", docs.join(format!("\n{double_offset}").as_str()))?;
                }

                writeln!(buffer, "{double_offset}{},", Self::method_signature(name, "table".into(), &func.params, &func.returns, true)?.join(format!("\n{double_offset}").as_str()))?;
            }

            writeln!(buffer, "{single_offset}}},")?;
        }

        write!(buffer, "{current_offset}}}")?;

        Ok(())
    }
}

fn needs_escape(key: &str) -> bool {
    key.chars().any(|v| !v.is_alphanumeric() && v != '_')
}

fn escape_key(key: &str) -> String {
    if needs_escape(key) {
        format!(r#"["{key}"]"#)
    } else {
        key.to_string()
    }
}
