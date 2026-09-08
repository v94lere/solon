//! Document JSON de configuration HCS (schéma v2.x), limité aux champs utilisés par Monodon.
//!
//! Référence : https://learn.microsoft.com/en-us/virtualization/api/hcs/schemareference et
//! `internal/hcs/schema2` de `hcsshim`. Les champs optionnels ne sont pas sérialisés quand ils
//! valent `None` : HCS refuse certains champs selon la version de Windows, donc on n'émet que
//! ce qu'on utilise réellement.

use std::collections::BTreeMap;

use monodon_core::vm::VmConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ComputeSystemDocument {
    pub schema_version: SchemaVersion,
    pub owner: String,
    /// `false` : la machine survit à la fermeture de tous les handles, ce qui permet au service
    /// de redémarrer et de se rattacher sans couper les conteneurs.
    pub should_terminate_on_last_handle_closed: bool,
    pub virtual_machine: VirtualMachine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualMachine {
    /// Un `reboot` invité arrête la machine au lieu de la redémarrer : le service reprend la main.
    pub stop_on_reset: bool,
    pub chipset: Chipset,
    pub compute_topology: ComputeTopology,
    pub devices: Devices,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Chipset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux_kernel_direct: Option<LinuxKernelDirect>,
}

/// Boot direct du noyau (schéma ≥ 2.2), sans firmware ni chargeur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LinuxKernelDirect {
    pub kernel_file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_rd_path: Option<String>,
    pub kernel_cmd_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ComputeTopology {
    pub memory: Memory,
    pub processor: Processor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Memory {
    #[serde(rename = "SizeInMB")]
    pub size_in_mb: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_overcommit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_deferred_commit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_hot_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_cold_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_cold_discard_hint: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Processor {
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Devices {
    /// Contrôleurs SCSI, clé = numéro de contrôleur (`"0"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scsi: Option<BTreeMap<String, Scsi>>,
    /// Ports série, clé `"0"` = COM1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub com_ports: Option<BTreeMap<String, ComPort>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hv_socket: Option<HvSocket>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan9: Option<Plan9>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_adapters: Option<BTreeMap<String, NetworkAdapter>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Scsi {
    /// Clé = LUN.
    pub attachments: BTreeMap<String, Attachment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Attachment {
    #[serde(rename = "Type")]
    pub kind: AttachmentType,
    pub path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caching_mode: Option<CachingMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentType {
    VirtualDisk,
    Iso,
    PassThru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CachingMode {
    Uncached,
    Cached,
    ReadOnlyCached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ComPort {
    /// Named pipe créé par le processus de la machine (vmwp) en tant que **serveur** ;
    /// le diagnostic s'y connecte en client, comme avec une VM Hyper-V classique.
    pub named_pipe: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HvSocket {
    pub hv_socket_config: HvSocketSystemConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct HvSocketSystemConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_bind_security_descriptor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_connect_security_descriptor: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub service_table: BTreeMap<String, HvSocketServiceConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct HvSocketServiceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bind_security_descriptor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_security_descriptor: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_wildcard_binds: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Plan9 {
    pub shares: Vec<Plan9Share>,
}

/// Drapeaux `Plan9Share.Flags` (source : `hcsshim`).
pub mod plan9_flags {
    pub const READ_ONLY: u32 = 0x1;
    pub const LINUX_METADATA: u32 = 0x4;
    pub const CASE_SENSITIVE: u32 = 0x8;
    pub const RESTRICT_FILE_ACCESS: u32 = 0x10;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Plan9Share {
    pub name: String,
    pub access_name: String,
    pub path: String,
    pub port: u32,
    pub flags: u32,
}

impl Plan9Share {
    /// Partage HCS à partir d'un dossier hôte : métadonnées Linux activées (permissions POSIX
    /// stockées en attributs étendus NTFS), lecture seule si demandé.
    pub fn from_host_share(share: &monodon_core::vm::HostShare) -> Self {
        let mut flags = plan9_flags::LINUX_METADATA;
        if share.read_only {
            flags |= plan9_flags::READ_ONLY;
        }
        Self {
            name: share.name.clone(),
            access_name: share.name.clone(),
            path: share.host_path.to_string_lossy().into_owned(),
            port: share.port,
            flags,
        }
    }
}

/// Requête `HcsModifyComputeSystem` (ajout/retrait à chaud).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ModifySettingRequest<T> {
    pub resource_path: String,
    pub request_type: RequestType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestType {
    Add,
    Remove,
    Update,
}

pub const PLAN9_SHARES_RESOURCE_PATH: &str = "VirtualMachine/Devices/Plan9/Shares";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkAdapter {
    pub endpoint_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
}

/// Élément renvoyé par `HcsEnumerateComputeSystems`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ComputeSystemSummary {
    pub id: String,
    #[serde(default)]
    pub system_type: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub runtime_id: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

/// Descripteur de sécurité SDDL : accès complet pour SYSTEM et les Administrateurs.
pub const SDDL_SYSTEM_AND_ADMINS: &str = "D:P(A;;FA;;;SY)(A;;FA;;;BA)";

impl ComputeSystemDocument {
    /// Construit le document HCS d'une machine Monodon.
    pub fn from_config(config: &VmConfig) -> Self {
        let mut devices = Devices::default();

        if !config.disks.is_empty() {
            let attachments = config
                .disks
                .iter()
                .enumerate()
                .map(|(lun, disk)| {
                    (
                        lun.to_string(),
                        Attachment {
                            kind: AttachmentType::VirtualDisk,
                            path: disk.path.to_string_lossy().into_owned(),
                            read_only: disk.read_only,
                            caching_mode: None,
                        },
                    )
                })
                .collect();
            devices.scsi = Some(BTreeMap::from([("0".to_owned(), Scsi { attachments })]));
        }

        if let Some(pipe) = &config.serial_pipe {
            devices.com_ports = Some(BTreeMap::from([(
                "0".to_owned(),
                ComPort {
                    named_pipe: pipe.clone(),
                },
            )]));
        }

        // Toujours déclarer le périphérique Plan9, même sans partage : sans lui, l'ajout d'un partage
        // à chaud échoue avec ERROR_NOT_FOUND (0x80070490) (constaté au bloc 4).
        devices.plan9 = Some(Plan9 {
            shares: config
                .shares
                .iter()
                .map(Plan9Share::from_host_share)
                .collect(),
        });

        if let Some(nic) = &config.network_adapter {
            devices.network_adapters = Some(BTreeMap::from([(
                nic.endpoint_id.clone(),
                NetworkAdapter {
                    endpoint_id: nic.endpoint_id.clone(),
                    mac_address: nic.mac_address.clone(),
                },
            )]));
        }

        devices.hv_socket = Some(HvSocket {
            hv_socket_config: HvSocketSystemConfig {
                default_bind_security_descriptor: Some(SDDL_SYSTEM_AND_ADMINS.to_owned()),
                default_connect_security_descriptor: Some(SDDL_SYSTEM_AND_ADMINS.to_owned()),
                service_table: BTreeMap::new(),
            },
        });

        Self {
            schema_version: SchemaVersion { major: 2, minor: 2 },
            owner: crate::OWNER.to_owned(),
            should_terminate_on_last_handle_closed: false,
            virtual_machine: VirtualMachine {
                stop_on_reset: true,
                chipset: Chipset {
                    linux_kernel_direct: Some(LinuxKernelDirect {
                        kernel_file_path: config.kernel.to_string_lossy().into_owned(),
                        init_rd_path: Some(config.initrd.to_string_lossy().into_owned()),
                        kernel_cmd_line: config.cmdline.clone(),
                    }),
                },
                compute_topology: ComputeTopology {
                    memory: Memory {
                        size_in_mb: config.memory_mb,
                        allow_overcommit: Some(true),
                        enable_deferred_commit: Some(true),
                        // Comme WSL2 : l'hôte peut récupérer les pages froides et libérées de l'invité.
                        enable_hot_hint: Some(true),
                        enable_cold_hint: Some(true),
                        enable_cold_discard_hint: Some(true),
                    },
                    processor: Processor {
                        count: config.processors,
                    },
                },
                devices,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monodon_core::vm::DiskAttachment;
    use std::path::PathBuf;

    fn config() -> VmConfig {
        VmConfig {
            id: "0f4e2c1a-1111-2222-3333-444455556666".into(),
            name: "monodon".into(),
            kernel: PathBuf::from(r"C:\ProgramData\Monodon\image\vmlinuz"),
            initrd: PathBuf::from(r"C:\ProgramData\Monodon\image\initrd.img"),
            cmdline: "console=ttyS0 panic=-1".into(),
            memory_mb: 2048,
            processors: 2,
            disks: vec![
                DiskAttachment {
                    path: PathBuf::from(r"C:\ProgramData\Monodon\image\rootfs.vhdx"),
                    read_only: true,
                },
                DiskAttachment {
                    path: PathBuf::from(r"C:\ProgramData\Monodon\data.vhdx"),
                    read_only: false,
                },
            ],
            shares: vec![monodon_core::vm::HostShare {
                name: "c".into(),
                host_path: PathBuf::from(r"C:\"),
                port: 9000,
                read_only: false,
            }],
            serial_pipe: Some(r"\\.\pipe\monodon-com1".into()),
            network_adapter: None,
        }
    }

    #[test]
    fn partage_plan9_avec_metadonnees_linux() {
        let json = serde_json::to_value(ComputeSystemDocument::from_config(&config())).unwrap();
        let share = &json["VirtualMachine"]["Devices"]["Plan9"]["Shares"][0];
        assert_eq!(share["Name"], "c");
        assert_eq!(share["AccessName"], "c");
        assert_eq!(share["Path"], r"C:\");
        assert_eq!(share["Port"], 9000);
        assert_eq!(share["Flags"], plan9_flags::LINUX_METADATA);
    }

    #[test]
    fn requete_de_modification() {
        let req = ModifySettingRequest {
            resource_path: PLAN9_SHARES_RESOURCE_PATH.into(),
            request_type: RequestType::Add,
            settings: Some(Plan9Share {
                name: "x".into(),
                access_name: "x".into(),
                path: r"D:\".into(),
                port: 9001,
                flags: 4,
            }),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["ResourcePath"], "VirtualMachine/Devices/Plan9/Shares");
        assert_eq!(json["RequestType"], "Add");
        assert_eq!(json["Settings"]["Port"], 9001);
    }

    #[test]
    fn document_attendu_par_hcs() {
        let doc = ComputeSystemDocument::from_config(&config());
        let json: serde_json::Value = serde_json::to_value(&doc).unwrap();

        assert_eq!(json["SchemaVersion"]["Major"], 2);
        assert_eq!(json["Owner"], "Monodon");
        assert_eq!(json["ShouldTerminateOnLastHandleClosed"], false);
        let vm = &json["VirtualMachine"];
        assert_eq!(
            vm["Chipset"]["LinuxKernelDirect"]["KernelFilePath"],
            r"C:\ProgramData\Monodon\image\vmlinuz"
        );
        assert_eq!(
            vm["Chipset"]["LinuxKernelDirect"]["KernelCmdLine"],
            "console=ttyS0 panic=-1"
        );
        assert_eq!(vm["ComputeTopology"]["Memory"]["SizeInMB"], 2048);
        assert_eq!(vm["ComputeTopology"]["Memory"]["AllowOvercommit"], true);
        assert_eq!(
            vm["ComputeTopology"]["Memory"]["EnableColdDiscardHint"],
            true
        );
        assert_eq!(vm["ComputeTopology"]["Processor"]["Count"], 2);
        let scsi = &vm["Devices"]["Scsi"]["0"]["Attachments"];
        assert_eq!(scsi["0"]["Type"], "VirtualDisk");
        assert_eq!(scsi["0"]["ReadOnly"], true);
        assert!(
            scsi["1"].get("ReadOnly").is_none(),
            "ReadOnly=false n'est pas émis"
        );
        assert_eq!(
            vm["Devices"]["ComPorts"]["0"]["NamedPipe"],
            r"\\.\pipe\monodon-com1"
        );
        assert_eq!(
            vm["Devices"]["HvSocket"]["HvSocketConfig"]["DefaultBindSecurityDescriptor"],
            SDDL_SYSTEM_AND_ADMINS
        );
    }

    #[test]
    fn sans_disque_ni_console_les_sections_sont_absentes() {
        let mut cfg = config();
        cfg.disks.clear();
        cfg.serial_pipe = None;
        let json = serde_json::to_value(ComputeSystemDocument::from_config(&cfg)).unwrap();
        assert!(json["VirtualMachine"]["Devices"].get("Scsi").is_none());
        assert!(json["VirtualMachine"]["Devices"].get("ComPorts").is_none());
    }

    #[test]
    fn enumeration_se_desserialise() {
        let raw = r#"[{"Id":"abc","SystemType":"VirtualMachine","Owner":"Monodon","RuntimeId":"x","State":"Running"}]"#;
        let list: Vec<ComputeSystemSummary> = serde_json::from_str(raw).unwrap();
        assert_eq!(list[0].id, "abc");
        assert_eq!(list[0].state.as_deref(), Some("Running"));
    }
}
