use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    process::Command,
};

/// Runs rustfmt on the generated files - this is lifted from tonic-build
fn fmt<P>(out_dir: P)
where P: AsRef<Path> + Display {
    let dir = walk_files(&out_dir.as_ref().to_path_buf(), "rs");

    for entry in dir {
        let out = Command::new("rustfmt")
            .arg("--emit")
            .arg("files")
            .arg("--edition")
            .arg("2018")
            .arg(entry.to_str().unwrap())
            .output()
            .unwrap();

        if !out.status.success() {
            panic!("status: {} - {}", out.status, String::from_utf8_lossy(&out.stderr));
        }
    }
}

fn walk_files(search_path: &PathBuf, search_ext: &str) -> Vec<PathBuf> {
    let mut protos = Vec::new();
    let paths_iter = search_path
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .map(|dir| dir.path());

    for path in paths_iter {
        if path.is_file() && path.extension().filter(|ext| ext == &search_ext).is_some() {
            protos.push(path)
        } else if path.is_dir() {
            protos.extend(walk_files(&path, search_ext));
        }
    }

    protos
}

pub struct ProtoCompiler {
    out_dir: Option<PathBuf>,
    type_attributes: HashMap<&'static str, &'static str>,
    field_attributes: HashMap<&'static str, &'static str>,
    proto_paths: Vec<PathBuf>,
    include_paths: Vec<PathBuf>,
}

impl ProtoCompiler {
    pub fn new() -> Self {
        Self {
            out_dir: None,
            type_attributes: HashMap::new(),
            field_attributes: HashMap::new(),
            proto_paths: Vec::new(),
            include_paths: Vec::new(),
        }
    }

    pub fn out_dir<P>(&mut self, out_dir: P) -> &mut Self
    where P: AsRef<Path> {
        self.out_dir = Some(out_dir.as_ref().to_path_buf());
        self
    }

    pub fn add_type_attribute(&mut self, path: &'static str, attr: &'static str) -> &mut Self {
        self.type_attributes.insert(path, attr);
        self
    }

    pub fn add_field_attribute(&mut self, path: &'static str, attr: &'static str) -> &mut Self {
        self.field_attributes.insert(path, attr);
        self
    }

    pub fn proto_paths<P: AsRef<Path>>(&mut self, proto_paths: &[P]) -> &mut Self {
        self.proto_paths
            .extend(proto_paths.into_iter().map(|p| p.as_ref().to_path_buf()));
        self
    }

    pub fn include_paths<P: AsRef<Path>>(&mut self, include_paths: &[P]) -> &mut Self {
        self.include_paths
            .extend(include_paths.into_iter().map(|p| p.as_ref().to_path_buf()));
        self
    }

    pub fn compile(&mut self) -> Result<(), String> {
        if self.proto_paths.is_empty() {
            return Err("proto_path not specified".to_string());
        }

        self.include_paths.extend(self.proto_paths.clone());

        let protos = self.proto_paths.iter().fold(Vec::new(), |mut protos, path| {
            protos.extend(walk_files(&path, "proto"));
            protos
        });

        let mut config = prost_build::Config::new();

        for (k, v) in &self.type_attributes {
            config.type_attribute(k, v);
        }

        for (k, v) in &self.field_attributes {
            config.field_attribute(k, v);
        }

        let out_dir = self
            .out_dir
            .take()
            .unwrap_or_else(|| PathBuf::from(std::env::var("OUT_DIR").unwrap()));

        config.out_dir(out_dir.clone());

        config.compile_protos(&protos, &self.include_paths).map_err(|err| {
            // Side effect - print the error to stderr
            eprintln!("\n{}", err);
            format!("{}", err)
        })?;

        fmt(out_dir.to_str().expect("out_dir must be utf8"));

        Ok(())
    }
}
