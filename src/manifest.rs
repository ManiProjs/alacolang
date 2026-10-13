use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use miette::Result;

#[derive(Debug, Clone)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub edition: String,
    pub entry: PathBuf,
    pub dependencies: BTreeMap<String, String>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path).map_err(|error| {
            miette::miette!("cannot read package manifest '{}': {error}", path.display())
        })?;
        Self::parse(&source, path)
    }

    fn parse(source: &str, path: &Path) -> Result<Self> {
        let document: toml::Value = toml::from_str(source).map_err(|error| {
            miette::miette!("invalid package manifest '{}': {error}", path.display())
        })?;
        let package = document
            .get("package")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| miette::miette!("manifest must contain a [package] table"))?;

        let name = required_string(package, "name")?;
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(miette::miette!(
                "invalid package name '{name}'; use ASCII letters, digits, '-' or '_'"
            ));
        }

        let version = required_string(package, "version")?;
        if version.split('.').count() < 3
            || !version.split('.').take(3).all(|part| {
                let numeric = part.split(['-', '+']).next().unwrap_or_default();
                !numeric.is_empty() && numeric.chars().all(|c| c.is_ascii_digit())
            })
        {
            return Err(miette::miette!(
                "invalid package version '{version}'; expected a semantic version such as 0.1.0"
            ));
        }

        let edition = package
            .get("edition")
            .and_then(toml::Value::as_str)
            .unwrap_or("2026")
            .to_owned();
        if edition != "2026" {
            return Err(miette::miette!(
                "unsupported Alaco edition '{edition}'; currently supported edition is '2026'"
            ));
        }

        let entry = package
            .get("entry")
            .and_then(toml::Value::as_str)
            .unwrap_or("src/main.aco");
        let entry = PathBuf::from(entry);
        if entry.is_absolute()
            || entry
                .components()
                .any(|component| component == std::path::Component::ParentDir)
        {
            return Err(miette::miette!(
                "package entry '{}' must be a relative path inside the package",
                entry.display()
            ));
        }
        if entry.extension().and_then(|ext| ext.to_str()) != Some("aco") {
            return Err(miette::miette!("package entry must point to an .aco file"));
        }

        let mut dependencies = BTreeMap::new();
        if let Some(deps) = document.get("dependencies") {
            let deps = deps.as_table().ok_or_else(|| {
                miette::miette!("[dependencies] must be a table of dependency declarations")
            })?;
            for (dependency_name, declaration) in deps {
                if dependency_name.is_empty()
                    || !dependency_name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    return Err(miette::miette!(
                        "invalid dependency name '{dependency_name}'"
                    ));
                }
                let description = if let Some(version) = declaration.as_str() {
                    version.to_owned()
                } else if let Some(table) = declaration.as_table() {
                    let local_path = table.get("path").and_then(toml::Value::as_str);
                    let version = table.get("version").and_then(toml::Value::as_str);
                    if let Some(local_path) = local_path {
                        format!("path = \"{local_path}\"")
                    } else if let Some(version) = version {
                        version.to_owned()
                    } else {
                        return Err(miette::miette!(
                            "dependency '{dependency_name}' must specify a version string or a 'path'/'version' field"
                        ));
                    }
                } else {
                    return Err(miette::miette!(
                        "dependency '{dependency_name}' must be a version string or an inline table"
                    ));
                };
                dependencies.insert(dependency_name.clone(), description);
            }
        }

        Ok(Self {
            name,
            version,
            edition,
            entry,
            dependencies,
        })
    }
}

fn required_string(table: &toml::map::Map<String, toml::Value>, key: &str) -> Result<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| miette::miette!("[package] must define a non-empty string '{key}'"))
}

#[cfg(test)]
mod tests {
    use super::Manifest;
    use std::path::Path;

    #[test]
    fn parses_package_metadata_entry_and_dependencies() {
        let manifest = Manifest::parse(
            "[package]\nname='demo'\nversion='1.2.3'\nentry='app/main.aco'\n[dependencies]\ncolors='^1.0'\nlocal={ path='../local' }\n",
            Path::new("Alaco.toml"),
        ).unwrap();
        assert_eq!(manifest.name, "demo");
        assert_eq!(manifest.version, "1.2.3");
        assert_eq!(manifest.entry, Path::new("app/main.aco"));
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.dependencies["colors"], "^1.0");
        assert_eq!(manifest.dependencies["local"], "path = \"../local\"");
    }

    #[test]
    fn rejects_unsafe_entry_and_invalid_versions() {
        let unsafe_entry = Manifest::parse(
            "[package]\nname='demo'\nversion='1.0.0'\nentry='../main.aco'\n",
            Path::new("Alaco.toml"),
        );
        assert!(unsafe_entry.is_err());

        let invalid_version = Manifest::parse(
            "[package]\nname='demo'\nversion='next'\n",
            Path::new("Alaco.toml"),
        );
        assert!(invalid_version.is_err());
    }
}
