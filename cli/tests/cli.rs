// Copyright 2019 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// This is a regression test for exonum configuration.

use exonum::{api::backends::actix::AllowOrigin, blockchain::ValidatorKeys, crypto::gen_keypair};
use exonum_cli::{
    command::{
        finalize::Finalize, generate_config::GenerateConfig, generate_template::GenerateTemplate,
    },
        run::Run, Command, ExonumCommand, StandardResult,
    config::{GeneralConfig, NodePrivateConfig, NodePublicConfig},
    io::{load_config_file, save_config_file},
    password::DEFAULT_MASTER_PASS_ENV_VAR,
};
use exonum_supervisor::mode::Mode as SupervisorMode;
use serde_derive::*;
use structopt::StructOpt;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    panic,
    path::{Path, PathBuf},
    str::FromStr,
};

#[derive(Debug)]
struct ConfigSpec {
    expected_root_dir: PathBuf,
    output_root_dir: tempfile::TempDir,
    validators_count: usize,
}

impl ConfigSpec {
    const CONFIG_TESTDATA_FOLDER: &'static str =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/testdata/config");

    fn new(root_dir: impl AsRef<Path>, validators_count: usize) -> Self {
        Self {
            expected_root_dir: root_dir.as_ref().to_owned(),
            output_root_dir: tempfile::tempdir().unwrap(),
            validators_count,
        }
    }

    fn new_without_pass() -> Self {
        let root_dir = PathBuf::from(Self::CONFIG_TESTDATA_FOLDER).join("without_pass");
        Self::new(root_dir, 4)
    }

    fn new_with_pass() -> Self {
        let root_dir = PathBuf::from(Self::CONFIG_TESTDATA_FOLDER).join("with_pass");
        Self::new(root_dir, 1)
    }

    fn new_more_validators() -> Self {
        let root_dir = PathBuf::from(Self::CONFIG_TESTDATA_FOLDER).join("more_validators");
        Self::new(root_dir, 4)
    }

    fn command(&self, name: &str) -> ArgsBuilder {
        ArgsBuilder {
            args: vec!["exonum-config-test".into(), name.into()],
        }
    }

    fn copy_node_config_to_output(&self, index: usize) {
        let src = self.expected_node_config_dir(index);
        let dest = self.output_node_config_dir(index);
        fs::create_dir_all(&dest).unwrap();

        ["pub.toml", "sec.toml", "master.key.toml"]
            .iter()
            .try_for_each(|file| copy_secured(src.join(file), dest.join(file)))
            .expect("Can't copy file");
    }

    fn output_dir(&self) -> PathBuf {
        self.output_root_dir.as_ref().join("cfg")
    }

    fn output_template_file(&self) -> PathBuf {
        self.output_dir().join("template.toml")
    }

    fn output_node_config_dir(&self, index: usize) -> PathBuf {
        self.output_dir().join(index.to_string())
    }

    fn output_private_config(&self, index: usize) -> PathBuf {
        self.output_node_config_dir(index).join("sec.toml")
    }

    fn output_public_config(&self, index: usize) -> PathBuf {
        self.output_node_config_dir(index).join("pub.toml")
    }

    fn output_pub_configs(&self) -> Vec<PathBuf> {
        (0..self.validators_count)
            .map(|i| self.output_public_config(i))
            .collect()
    }

    fn output_node_config(&self, index: usize) -> PathBuf {
        self.output_node_config_dir(index).join("node.toml")
    }

    fn expected_dir(&self) -> PathBuf {
        self.expected_root_dir.join("cfg")
    }

    fn expected_template_file(&self) -> PathBuf {
        self.expected_dir().join("template.toml")
    }

    fn expected_node_config_dir(&self, index: usize) -> PathBuf {
        self.expected_dir().join(index.to_string())
    }

    fn expected_node_config_file(&self, index: usize) -> PathBuf {
        self.expected_node_config_dir(index).join("node.toml")
    }

    fn expected_pub_config(&self, index: usize) -> PathBuf {
        self.expected_node_config_dir(index).join("pub.toml")
    }

    fn expected_pub_configs(&self) -> Vec<PathBuf> {
        (0..self.validators_count)
            .map(|i| self.expected_pub_config(i))
            .collect()
    }
}

#[derive(Debug)]
struct ArgsBuilder {
    args: Vec<OsString>,
}

impl ArgsBuilder {
    fn with_arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    fn with_args(mut self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        for arg in args {
            self.args.push(arg.into())
        }
        self
    }

    fn with_named_arg(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.args.push(name.into());
        self.args.push(value.into());
        self
    }

    fn run(self) -> Result<StandardResult, failure::Error> {
        let command = <Command as StructOpt>::from_iter_safe(self.args).unwrap();
        command.execute()
    }
}

fn is_run_node_config(result: StandardResult) -> bool {
    if let StandardResult::Run(_) = result {
        true
    } else {
        false
    }
}

fn touch(path: impl AsRef<Path>) {
    OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .unwrap();
}

fn copy_secured(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<(), failure::Error> {
    let mut source_file = fs::File::open(&from)?;

    let mut destination_file = {
        let mut open_options = OpenOptions::new();
        open_options.create(true).write(true);
        #[cfg(unix)]
        open_options.mode(0o600);
        open_options.open(&to)?
    };

    std::io::copy(&mut source_file, &mut destination_file)?;
    Ok(())
}

fn assert_config_files_eq(path_1: impl AsRef<Path>, path_2: impl AsRef<Path>) {
    let cfg_1: toml::Value = load_config_file(&path_1).unwrap();
    let cfg_2: toml::Value = load_config_file(&path_2).unwrap();
    assert_eq!(
        cfg_1,
        cfg_2,
        "file {:?} doesn't match with {:?}",
        path_1.as_ref(),
        path_2.as_ref()
    );
}

#[test]
fn test_allow_origin_toml() {
    fn check(text: &str, allow_origin: AllowOrigin) {
        #[derive(Serialize, Deserialize)]
        struct Config {
            allow_origin: AllowOrigin,
        }
        let config_toml = format!("allow_origin = {}\n", text);
        let config: Config = ::toml::from_str(&config_toml).unwrap();
        assert_eq!(config.allow_origin, allow_origin);
        assert_eq!(::toml::to_string(&config).unwrap(), config_toml);
    }

    check(r#""*""#, AllowOrigin::Any);
    check(
        r#""http://example.com""#,
        AllowOrigin::Whitelist(vec!["http://example.com".to_string()]),
    );
    check(
        r#"["http://a.org", "http://b.org"]"#,
        AllowOrigin::Whitelist(vec!["http://a.org".to_string(), "http://b.org".to_string()]),
    );
}

#[test]
fn test_generate_template() {
    let env = ConfigSpec::new_without_pass();
    let output_template_file = env.output_template_file();
    env.command("generate-template")
        .with_arg(&output_template_file)
        .with_named_arg("--validators-count", env.validators_count.to_string())
        .with_named_arg("--supervisor-mode", "simple")
        .run()
        .unwrap();
    assert_config_files_eq(&output_template_file, env.expected_template_file());
}

#[test]
fn test_generate_template_simple_supervisor() {
    let env = ConfigSpec::new_without_pass();
    let output_template_file = env.output_template_file();
    env.command("generate-template")
        .with_arg(&output_template_file)
        .with_named_arg("--validators-count", env.validators_count.to_string())
        .with_named_arg("--supervisor-mode", "simple")
        .run()
        .unwrap();
    assert_config_files_eq(&output_template_file, env.expected_template_file());
}

#[test]
fn test_generate_config_key_files() {
    let env = ConfigSpec::new_without_pass();

    env.command("generate-config")
        .with_arg(&env.expected_template_file())
        .with_arg(&env.output_node_config_dir(0))
        .with_named_arg("-a", "0.0.0.0:8000")
        .with_arg("--no-password")
        .run()
        .unwrap();

    let private_cfg: toml::Value = load_config_file(&env.output_private_config(0)).unwrap();
    assert_eq!(private_cfg["master_key_path"], "master.key.toml".into());
}

#[test]
fn master_key_path_current_dir() {
    let env = ConfigSpec::new_without_pass();

    let temp_dir = TempDir::new().unwrap().into_path();
    env::set_current_dir(temp_dir).unwrap();

    env.command("generate-config")
        .with_arg(&env.expected_template_file())
        .with_arg(&env.output_node_config_dir(0))
        .with_named_arg("-a", "0.0.0.0:8000")
        .with_arg("--no-password")
        .with_named_arg("--master-key-path", ".")
        .run()
        .unwrap();

    let current_dir = std::env::current_dir().unwrap();
    let expected_path = current_dir.join("master.key.toml");

    let private_cfg: toml::Value = load_config_file(&env.output_private_config(0)).unwrap();
    assert_eq!(
        private_cfg["master_key_path"],
        expected_path.to_str().unwrap().into()
    );
}

#[test]
#[should_panic]
fn invalid_master_key_path() {
    let env = ConfigSpec::new_without_pass();

    env.command("generate-config")
        .with_arg(&env.expected_template_file())
        .with_arg(&env.output_node_config_dir(0))
        .with_named_arg("-a", "0.0.0.0:8000")
        .with_arg("--no-password")
        .with_named_arg("--master-key-path", "./..not-valid/path/")
        .run()
        .unwrap();
}

#[test]
fn test_generate_config_ipv4() {
    let env = ConfigSpec::new_without_pass();
    env.command("generate-config")
        .with_arg(&env.expected_template_file())
        .with_arg(&env.output_node_config_dir(0))
        .with_named_arg("-a", "127.0.0.1")
        .with_arg("--no-password")
        .run()
        .unwrap();
}

#[test]
fn test_generate_config_ipv6() {
    let env = ConfigSpec::new_without_pass();
    env.command("generate-config")
        .with_arg(&env.expected_template_file())
        .with_arg(&env.output_node_config_dir(0))
        .with_named_arg("-a", "::1")
        .with_arg("--no-password")
        .run()
        .unwrap();
}

#[test]
fn test_finalize_run_without_pass() {
    let env = ConfigSpec::new_without_pass();
    for i in 0..env.validators_count {
        env.copy_node_config_to_output(i);
        let node_config = env.output_node_config(i);
        env.command("finalize")
            .with_arg(env.output_private_config(i))
            .with_arg(&node_config)
            .with_arg("--public-configs")
            .with_args(env.expected_pub_configs())
            .run()
            .unwrap();
        assert_config_files_eq(&node_config, env.expected_node_config_file(i));

        let feedback = env
            .command("run")
            .with_named_arg("-c", &node_config)
            .with_named_arg("-d", env.output_dir().join("foo"))
            .with_named_arg("--master-key-pass", "pass:")
            .run();
        assert!(is_run_node_config(feedback.unwrap()));
    }
}

#[test]
fn test_finalize_run_with_pass() {
    let env = ConfigSpec::new_with_pass();

    env::set_var(DEFAULT_MASTER_PASS_ENV_VAR, "some passphrase");
    env.copy_node_config_to_output(0);
    let node_config = env.output_node_config(0);
    env.command("finalize")
        .with_arg(env.output_private_config(0))
        .with_arg(&node_config)
        .with_arg("--public-configs")
        .with_args(env.expected_pub_configs())
        .run()
        .unwrap();
    assert_config_files_eq(&node_config, env.expected_node_config_file(0));

    let feedback = env
        .command("run")
        .with_named_arg("-c", &node_config)
        .with_named_arg("-d", env.output_dir().join("foo"))
        .with_named_arg("--master-key-pass", "env")
        .run();
    assert!(is_run_node_config(feedback.unwrap()));
}

#[test]
#[should_panic(
    expected = "The number of validators (3) does not match the number of validators keys (4)."
)]
fn test_more_validators_count() {
    let env = ConfigSpec::new_more_validators();

    let node_config = env.output_node_config(0);
    env.copy_node_config_to_output(0);
    env.command("finalize")
        .with_arg(env.output_private_config(0))
        .with_arg(&node_config)
        .with_arg("--public-configs")
        .with_args(env.expected_pub_configs())
        .run()
        .unwrap();
}

#[test]
fn test_full_workflow() {
    let env = ConfigSpec::new("", 4);

    let output_template_file = env.output_template_file();
    env.command("generate-template")
        .with_arg(&output_template_file)
        .with_named_arg("--validators-count", env.validators_count.to_string())
        .with_named_arg("--supervisor-mode", "simple")
        .run()
        .unwrap();

    for i in 0..env.validators_count {
        env.command("generate-config")
            .with_arg(&output_template_file)
            .with_arg(&env.output_node_config_dir(i))
            .with_named_arg("-a", format!("0.0.0.0:{}", 8000 + i))
            .with_named_arg("--master-key-pass", "pass:12345678")
            .run()
            .unwrap();
    }

    env::set_var("EXONUM_MASTER_PASS", "12345678");
    for i in 0..env.validators_count {
        let node_config = env.output_node_config(i);
        env.command("finalize")
            .with_arg(env.output_private_config(i))
            .with_arg(&node_config)
            .with_arg("--public-configs")
            .with_args(env.output_pub_configs())
            .run()
            .unwrap();

        let feedback = env
            .command("run")
            .with_named_arg("-c", &node_config)
            .with_named_arg("-d", env.output_dir().join("foo"))
            .with_named_arg("--master-key-pass", "env")
            .run();
        assert!(is_run_node_config(feedback.unwrap()));
    }
}

#[test]
fn test_run_dev() {
    let env = ConfigSpec::new_without_pass();

    let artifacts_dir = env.output_dir().join("artifacts");
    // Mocks existence of old DB files that are supposed to be cleaned up.
    let db_dir = artifacts_dir.join("db");
    fs::create_dir_all(&db_dir).unwrap();
    let old_db_file = db_dir.join("content.foo");
    touch(&old_db_file);
    // Checks run-dev command.
    let feedback = env
        .command("run-dev")
        .with_arg("-a")
        .with_arg(&artifacts_dir)
        .run();
    assert!(is_run_node_config(feedback.unwrap()));
    // Tests cleaning up.
    assert!(!old_db_file.exists());
}

#[test]
fn test_clear_cache() {
    let env = ConfigSpec::new_without_pass();
    let db_path = env.output_dir().join("db0");

    env.command("maintenance")
        .with_named_arg("--node-config", &env.expected_node_config_file(0))
        .with_named_arg("--db-path", &db_path)
        .with_arg("clear-cache")
        .run()
        .unwrap();
}

#[test]
fn run_node_with_simple_supervisor() {
    run_node_with_supervisor(&SupervisorMode::Simple).unwrap();
}

#[test]
fn run_node_with_decentralized_supervisor() {
    run_node_with_supervisor(&SupervisorMode::Decentralized).unwrap();
}

#[test]
fn different_supervisor_modes_in_public_configs() -> Result<(), failure::Error> {
    let pub_config_1 = public_config(SupervisorMode::Simple);
    let pub_config_2 = public_config(SupervisorMode::Decentralized);
    let private_config = NodePrivateConfig {
        listen_address: "127.0.0.1:5400".parse().unwrap(),
        external_address: "127.0.0.1:5400".to_string(),
        master_key_path: Default::default(),
        api: Default::default(),
        network: Default::default(),
        mempool: Default::default(),
        database: Default::default(),
        thread_pool_size: None,
        connect_list: Default::default(),
        keys: Default::default(),
    };

    let testnet_dir = tempfile::tempdir()?;
    let pub_config_1_path = testnet_dir.path().join("pub1.toml");
    let pub_config_2_path = testnet_dir.path().join("pub2.toml");
    let private_config_path = testnet_dir.path().join("sec.toml");

    save_config_file(&pub_config_1, &pub_config_1_path)?;
    save_config_file(&pub_config_2, &pub_config_2_path)?;
    save_config_file(&private_config, &private_config_path)?;

    let finalize = Finalize {
        private_config_path: testnet_dir.path().join("sec.toml"),
        output_config_path: testnet_dir.path().join("node.toml"),
        public_configs: vec![pub_config_1_path, pub_config_2_path],
        public_api_address: None,
        private_api_address: None,
        public_allow_origin: None,
        private_allow_origin: None,
    };
    let err = finalize.execute().err().unwrap();
    assert!(err
        .to_string()
        .contains("Found public configs with different general configuration."));
    Ok(())
}

fn public_config(supervisor_mode: SupervisorMode) -> NodePublicConfig {
    let keys = ValidatorKeys {
        consensus_key: gen_keypair().0,
        service_key: gen_keypair().0,
    };
    NodePublicConfig {
        consensus: Default::default(),
        general: GeneralConfig {
            validators_count: 2,
            supervisor_mode,
        },
        validator_keys: Some(keys),
    }
}

fn run_node_with_supervisor(supervisor_mode: &SupervisorMode) -> Result<(), failure::Error> {
    let testnet_dir = tempfile::tempdir()?;

    let common_config_path = testnet_dir.path().join("common.toml");

    let generate_template = GenerateTemplate {
        common_config: common_config_path.clone(),
        validators_count: 1,
        supervisor_mode: supervisor_mode.clone(),
    };
    generate_template.execute()?;

    let generate_config = GenerateConfig {
        common_config: common_config_path.clone(),
        output_dir: testnet_dir.path().to_owned(),
        peer_address: "127.0.0.1:5400".parse().unwrap(),
        listen_address: None,
        no_password: true,
        master_key_pass: None,
        master_key_path: None,
    };
    let (public_config, secret_config) = match generate_config.execute()? {
        StandardResult::GenerateConfig {
            public_config_path,
            private_config_path: secret_config_path,
            ..
        } => (public_config_path, secret_config_path),
        _ => unreachable!("Invalid result of generate-config"),
    };

    let node_config_path = testnet_dir.path().join("node.toml");

    let finalize = Finalize {
        private_config_path: secret_config,
        output_config_path: node_config_path.clone(),
        public_configs: vec![public_config],
        public_api_address: None,
        private_api_address: None,
        public_allow_origin: None,
        private_allow_origin: None,
    };
    finalize.execute()?;

    let run = Run {
        node_config: node_config_path.clone(),
        db_path: testnet_dir.path().to_owned(),
        public_api_address: None,
        private_api_address: None,
        master_key_pass: Some(FromStr::from_str("pass:")?),
    };

    if let StandardResult::Run(config) = run.execute()? {
        assert_eq!(
            config.node_config.public_config.general.supervisor_mode,
            *supervisor_mode
        );
    } else {
        unreachable!("Invalid result of run");
    }

    Ok(())
}
