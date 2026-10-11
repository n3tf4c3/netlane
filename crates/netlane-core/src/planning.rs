use crate::{Adapter, RouteMode, RoutingPolicy, resolve_interface};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedRule {
    pub application: String,
    pub executable: Option<String>,
    pub mode: RouteMode,
    pub interface_id: Option<String>,
    pub luid: u64,
    pub ipv4: bool,
    pub ipv6: bool,
    pub detail: String,
}
pub fn plan(policies: &[RoutingPolicy], adapters: &[Adapter]) -> Vec<PlannedRule> {
    policies
        .iter()
        .map(|p| {
            let target = resolve_interface(p, adapters);
            let actionable = p.enabled && p.route_mode != RouteMode::Automatic;
            let detail = if !actionable {
                "Windows decide; remover política NetLane.".to_owned()
            } else if p.route_mode == RouteMode::Blocked {
                "Bloqueio IPv4/IPv6 solicitado.".to_owned()
            } else if let Some(a) = target {
                format!(
                    "Saída solicitada: {}. Tráfego ainda não verificado.",
                    a.name
                )
            } else {
                "Interface selecionada ausente/desconectada; política retirada.".to_owned()
            };
            PlannedRule {
                application: p.application_id.clone(),
                executable: p.executable_path.clone(),
                mode: if actionable && (p.route_mode == RouteMode::Blocked || target.is_some()) {
                    p.route_mode
                } else {
                    RouteMode::Automatic
                },
                interface_id: target.map(|a| a.id.clone()),
                luid: target.map_or(0, |a| a.luid),
                ipv4: target.is_none_or(|a| a.ipv4),
                ipv6: target.is_none_or(|a| a.ipv6),
                detail,
            }
        })
        .collect()
}
/// Actionable rules whose executable is not a present local file fall back to Windows'
/// choice. `exists` is never called for a remote or device path.
pub fn retire_missing(rules: &mut [PlannedRule], exists: impl Fn(&str) -> bool) {
    for rule in rules {
        if rule.mode != RouteMode::Automatic
            && rule
                .executable
                .as_deref()
                .is_none_or(|p| !crate::is_local_path(p) || !exists(p))
        {
            rule.mode = RouteMode::Automatic;
            rule.detail = "Executável ausente ou remoto; política retirada.".to_owned();
        }
    }
}
pub fn expand_related(
    policies: &[RoutingPolicy],
    exists: impl Fn(&str) -> bool,
) -> Vec<RoutingPolicy> {
    let mut result = policies.to_vec();
    let mut explicit: std::collections::HashSet<String> = policies
        .iter()
        .flat_map(|p| {
            [
                p.application_id.to_lowercase(),
                crate::file_name(p.executable_path.as_deref().unwrap_or(&p.application_id))
                    .to_lowercase(),
            ]
        })
        .collect();
    for policy in policies {
        let Some(path) = policy.executable_path.as_deref() else {
            continue;
        };
        if !policy.enabled
            || !policy.include_related_executables
            || policy.route_mode == RouteMode::Automatic
            || !crate::file_name(path).eq_ignore_ascii_case("steam.exe")
        {
            continue;
        }
        // The elevated agent checks candidates with `exists`: never for a remote root.
        if !crate::is_local_path(path) {
            continue;
        }
        let Some(root) = path.rsplit_once(['\\', '/']).map(|p| p.0) else {
            continue;
        };
        for name in [
            "steamwebhelper.exe",
            "steamservice.exe",
            "steamerrorreporter.exe",
            "gameoverlayui.exe",
        ] {
            if explicit.contains(name) {
                continue;
            }
            let mut folders = vec!["", "bin", "cef", "bin\\cef"];
            if name == "steamwebhelper.exe" {
                folders.extend([
                    "bin\\cef\\cef.win64",
                    "bin\\cef\\cef.win7x64",
                    "bin\\cef\\cef.win7",
                ]);
            }
            if let Some(candidate) = folders
                .into_iter()
                .map(|f| {
                    if f.is_empty() {
                        format!("{root}\\{name}")
                    } else {
                        format!("{root}\\{f}\\{name}")
                    }
                })
                .find(|p| exists(p))
            {
                let mut related = policy.clone();
                related.application_id = name.to_owned();
                related.executable_path = Some(candidate);
                related.include_related_executables = false;
                related.additional_data.clear();
                result.push(related);
                explicit.insert(name.to_owned());
            }
        }
    }
    result
}
