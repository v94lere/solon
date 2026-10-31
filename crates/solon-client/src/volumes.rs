//! Copie des volumes d'un projet Compose vers un autre : repartir sur une branche avec les données
//! d'une autre, sans toucher à l'original.

use crate::control::{exec, shell_quote};

/// Copie les volumes nommés du projet `from` vers le projet `to` (`<to>_<suffixe>`), dans la machine,
/// avec les étiquettes que Compose attend. Les volumes de destination déjà présents sont laissés tels
/// quels (jamais écrasés). Renvoie le nombre de volumes copiés.
pub async fn clone_project(from: &str, to: &str) -> Result<usize, String> {
    let (_, out, _) = exec(
        format!(
            "docker volume ls --filter label=com.docker.compose.project={} --format '{{{{.Name}}}}'",
            shell_quote(from)
        ),
        30,
    )
    .await?;
    let names: Vec<String> = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    let version = exec("docker compose version --short".into(), 30)
        .await
        .ok()
        .map(|(_, v, _)| v.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "2.0.0".into());
    let mut script = String::new();
    for name in &names {
        let suffix = name.strip_prefix(&format!("{from}_")).unwrap_or(name);
        let dest = format!("{to}_{suffix}");
        script.push_str(&format!(
            "docker volume inspect {d} >/dev/null 2>&1 || (docker volume create --label com.docker.compose.project={p} --label com.docker.compose.volume={s} --label com.docker.compose.version={ver} {d} >/dev/null && cp -a /var/lib/docker/volumes/{n}/_data/. /var/lib/docker/volumes/{d}/_data/ && echo cloned {d})\n",
            d = shell_quote(&dest),
            p = shell_quote(to),
            s = shell_quote(suffix),
            ver = shell_quote(&version),
            n = shell_quote(name)
        ));
    }
    if script.is_empty() {
        return Ok(0);
    }
    let (code, out, err) = exec(script, 1800).await?;
    if code != 0 {
        return Err(format!("volume clone: {}", err.trim()));
    }
    Ok(out.lines().filter(|l| l.starts_with("cloned ")).count())
}
