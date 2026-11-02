//! The reuse rules of a scope's VM beyond an equal spec (EAC-FR-HXDN,
//! EAC-FR-UEFI, EAC-FR-JRVY, EAC-FR-QULL, EAC-FR-PWLA).

use super::*;

// EAC-FR-HXDN: a review and a merge_review launch never reuse a VM, even with
// an equal spec, and the old VM is stopped before the new one boots. A
// merge_work launch reuses it.
#[test]
fn a_review_kind_launch_always_boots_a_new_vm() {
    let fx = fixture();
    let s = settings();
    let spec = spec("agent:1", "/w/kinds");
    let args: Vec<String> = Vec::new();
    for (kind, expected) in [
        (TurnKind::Review, vec!["boot", "stop", "boot"]),
        (TurnKind::MergeReview, vec!["boot", "stop", "boot"]),
        (TurnKind::MergeWork, vec!["boot", "refresh"]),
    ] {
        let runtime = RecordingSandbox::with_boots(Vec::new());
        let pool = SandboxPool::new(runtime.clone());
        block_on(async {
            for _ in 0..2 {
                pool.launch(&Silent, launch_kind(&fx, &spec, &s, kind), process(&args))
                    .await
                    .unwrap();
            }
        });
        assert_eq!(lifecycle(&runtime), expected, "{kind:?}");
    }
}

// EAC-FR-HXDN: a read/write source made again at the same path is another
// directory, so the next launch boots a new VM.
#[test]
fn a_directory_made_again_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/again");
    let source = PathBuf::from(&spec.mounts[0].source);
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        // Renamed aside rather than deleted, so the inode cannot be given back.
        std::fs::rename(&source, source.with_file_name("again-aside")).unwrap();
        std::fs::create_dir(&source).unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
}

// EAC-FR-HXDN: a read/write source that is not there has no identity, which is
// never the same, so the launch boots a new VM rather than reusing one.
#[test]
fn a_source_with_no_identity_is_never_reused() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let absent = real_dir("/w/absent-parent").to_string() + "/absent";
    let spec = SandboxSpec::new(
        "agent:1".into(),
        OWNER.into(),
        &s,
        vec![SandboxMount { source: absent.clone(), target: absent, read_only: false }],
    );
    let args: Vec<String> = Vec::new();
    block_on(async {
        for _ in 0..2 {
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args))
                .await
                .unwrap();
        }
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
}

// EAC-FR-UEFI, EAC-FR-KTWO: the owner is part of what a VM boots with, so a
// launch for another owner boots a new VM.
#[test]
fn another_owner_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let first = spec("agent:1", "/w/owner");
    let mut other = first.clone();
    other.owner = "1001:1001".into();
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &first, &s, None), process(&args)).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &other, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
    assert_eq!(pool.booted("vm-a").map(|spec| spec.owner), Some("1001:1001".to_string()));
}

// EAC-FR-JRVY: a VM whose caches cannot be dropped is not reused: it is stopped
// and a new one boots before the launch.
#[test]
fn a_failed_cache_drop_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    *runtime.refresh_error.lock().unwrap() = Some("no root".into());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("agent:1", "/w/refresh");
    let args = vec!["-p".to_string()];
    block_on(async {
        for _ in 0..2 {
            pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args))
                .await
                .unwrap();
        }
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "refresh", "stop", "boot"]);
    assert_eq!(runtime.calls().last(), Some(&Call::Run("vm-a".into(), args)));
}

// EAC-FR-QULL: an image the verified Docker backend does not hold boots from
// the configured reference through a registry, and nothing is exported.
#[test]
fn an_image_docker_does_not_hold_boots_from_a_registry() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let spec = spec("registry.example/agent:1", "/w/registry");
    let missing = Arc::new(StubExporter {
        id: Err("no such image".into()),
        exports: StdMutex::new(0),
        exported: StdMutex::new(Vec::new()),
        fail_export: false,
    });
    let args: Vec<String> = Vec::new();
    block_on(async {
        let exporter: Arc<dyn ImageExporter> = missing.clone();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, Some(exporter)), process(&args))
            .await
            .unwrap();
    });
    assert_eq!(
        runtime.calls()[0],
        Call::Boot("vm-a".into(), BootImage::Registry("registry.example/agent:1".into()))
    );
    assert_eq!(*missing.exports.lock().unwrap(), 0);
}

// EAC-FR-PWLA: the image's `Env` comes first, a launch variable replaces an
// image variable of the same name, the last of two equal image names wins, a
// value keeps every `=` after the first, and an entry with no `=` is left out.
#[test]
fn the_process_environment_puts_the_launch_over_the_image() {
    let image = vec![
        "PATH=/usr/local/cargo/bin:/usr/bin".to_string(),
        "HOME=/home/agent".to_string(),
        "GIT_OPTIONAL_LOCKS=1".to_string(),
        "OPTS=a=b".to_string(),
        "EMPTY=".to_string(),
        "NOVALUE".to_string(),
        "HOME=/home/other".to_string(),
    ];
    let launch = vec![
        ("GIT_OPTIONAL_LOCKS".to_string(), SecretString::new("0".to_string())),
        ("TOKEN".to_string(), SecretString::new("secret".to_string())),
    ];
    let composed: Vec<(String, String)> = compose_env(&image, &launch)
        .into_iter()
        .map(|(name, value)| (name, value.expose().to_string()))
        .collect();
    let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()
    };
    assert_eq!(
        composed,
        pairs(&[
            ("PATH", "/usr/local/cargo/bin:/usr/bin"),
            ("OPTS", "a=b"),
            ("EMPTY", ""),
            ("HOME", "/home/other"),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("TOKEN", "secret"),
        ])
    );
}

// EAC-FR-HXDN: only read/write sources count. A read-only source made again at
// the same path does not reboot the VM.
#[test]
fn a_read_only_source_made_again_does_not_reboot() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let workspace = real_dir("/w/ro-parent");
    let read_only = real_dir("/w/ro-source");
    let spec = SandboxSpec::new(
        "agent:1".into(),
        OWNER.into(),
        &s,
        vec![
            SandboxMount { source: workspace.clone(), target: workspace, read_only: false },
            SandboxMount { source: read_only.clone(), target: read_only.clone(), read_only: true },
        ],
    );
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
        std::fs::rename(&read_only, format!("{read_only}-aside")).unwrap();
        std::fs::create_dir(&read_only).unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "refresh"]);
}

// EAC-FR-UEFI, EAC-FR-QDVN: the disk size is part of what a VM boots with, so
// a launch with another disk size boots a new VM.
#[test]
fn another_disk_size_boots_a_new_vm() {
    let fx = fixture();
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    let first = spec("agent:1", "/w/disk");
    let mut larger = first.clone();
    larger.disk_gib = 128;
    let args: Vec<String> = Vec::new();
    block_on(async {
        pool.launch(&Silent, launch(&fx, "vm-a", &first, &s, None), process(&args)).await.unwrap();
        pool.launch(&Silent, launch(&fx, "vm-a", &larger, &s, None), process(&args)).await.unwrap();
    });
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
    assert_eq!(pool.booted("vm-a").map(|spec| spec.disk_gib), Some(128));
}
