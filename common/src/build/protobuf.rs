use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt::Display,
    fs,
    fs::File,
    io,
    path::{Path, PathBuf},
    process::Command,
};

/// Runs rustfmt on the generated files - this is lifted from tonic-build
fn rustfmt<P>(out_dir: P)
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

fn walk_files<P: AsRef<Path>>(search_path: P, search_ext: &str) -> Vec<PathBuf> {
    let mut protos = Vec::new();
    let paths_iter = search_path
        .as_ref()
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

#[derive(Default)]
pub struct ProtobufCompiler {
    out_dir: Option<PathBuf>,
    type_attributes: HashMap<&'static str, &'static str>,
    field_attributes: HashMap<&'static str, &'static str>,
    proto_paths: Vec<PathBuf>,
    include_paths: Vec<PathBuf>,
    emit_rerun_if_changed_directives: bool,
    do_rustfmt: bool,
}

impl ProtobufCompiler {
    pub fn new() -> Self {
        Self {
            out_dir: None,
            type_attributes: HashMap::new(),
            field_attributes: HashMap::new(),
            proto_paths: Vec::new(),
            include_paths: Vec::new(),
            emit_rerun_if_changed_directives: false,
            do_rustfmt: false,
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

    pub fn perform_rustfmt(&mut self) -> &mut Self {
        self.do_rustfmt = true;
        self
    }

    pub fn proto_paths<P: AsRef<Path>>(&mut self, proto_paths: &[P]) -> &mut Self {
        self.proto_paths
            .extend(proto_paths.iter().map(|p| p.as_ref().to_path_buf()));
        self
    }

    pub fn emit_rerun_if_changed_directives(&mut self) -> &mut Self {
        self.emit_rerun_if_changed_directives = true;
        self
    }

    pub fn include_paths<P: AsRef<Path>>(&mut self, include_paths: &[P]) -> &mut Self {
        self.include_paths
            .extend(include_paths.iter().map(|p| p.as_ref().to_path_buf()));
        self
    }

    fn hash_file_contents<P: AsRef<Path>>(&self, file_path: P) -> Result<Vec<u8>, String> {
        let mut file = File::open(file_path).unwrap();
        let mut file_hash = Sha256::default();
        io::copy(&mut file, &mut file_hash).map_err(|err| format!("Failed to hash file: '{}'", err))?;
        Ok(file_hash.result().to_vec())
    }

    fn compare_and_move<P: AsRef<Path>>(&self, tmp_out_dir: P, out_dir: P) {
        let tmp_files = walk_files(tmp_out_dir, "rs");
        for tmp_file in tmp_files {
            let target_file = out_dir.as_ref().join(tmp_file.file_name().unwrap());
            if target_file.exists() {
                let tmp_hash = self.hash_file_contents(&tmp_file).unwrap();
                let target_hash = self.hash_file_contents(&target_file).unwrap();
                if tmp_hash != target_hash {
                    fs::rename(tmp_file, target_file).unwrap();
                }
            } else {
                fs::rename(tmp_file, target_file).unwrap();
            }
        }
    }

    pub fn compile(&mut self) -> Result<(), String> {
        if self.proto_paths.is_empty() {
            return Err("proto_path not specified".to_string());
        }

        let include_protos =
            self.include_paths
                .iter()
                .fold(Vec::with_capacity(self.include_paths.len()), |mut protos, path| {
                    protos.extend(walk_files(&path, "proto"));
                    protos
                });

        let protos = self
            .proto_paths
            .iter()
            .fold(Vec::with_capacity(self.proto_paths.len()), |mut protos, path| {
                protos.extend(walk_files(&path, "proto"));
                protos
            });

        self.include_paths.extend(self.proto_paths.clone());

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

        let tmp_out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("tmp_protos");
        fs::create_dir_all(&tmp_out_dir)
            .map_err(|err| format!("Failed to create temporary out dir because '{}'", err))?;

        config.out_dir(tmp_out_dir.clone());

        config.compile_protos(&protos, &self.include_paths).map_err(|err| {
            // Side effect - print the error to stderr
            eprintln!("\n{}", err);
            format!("{}", err)
        })?;

        if self.do_rustfmt {
            rustfmt(tmp_out_dir.to_str().expect("out_dir must be utf8"));
        }

        self.compare_and_move(&tmp_out_dir, &out_dir);

        fs::remove_dir_all(&tmp_out_dir).map_err(|err| format!("Failed to remove temporary dir: {}", err))?;

        if self.emit_rerun_if_changed_directives {
            protos.iter().chain(include_protos.iter()).for_each(|p| {
                println!("cargo:rerun-if-changed={}", p.to_string_lossy());
            });
        }

        Ok(())
    }
}
