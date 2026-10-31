//! Les conteneurs tels que `docker ps` les décrit, lus dans la machine par `Exec` : ce que l'icône
//! de la barre des tâches et le CLI affichent tous deux.

use crate::control::exec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub project: Option<String>,
    pub service: Option<String>,
    /// `running`, `exited`, `paused`…
    pub state: String,
    /// Texte de Docker (`Up 2 hours (healthy)`).
    pub status: String,
    pub ports: String,
}

impl Container {
    pub fn running(&self) -> bool {
        self.state == "running"
    }

    /// Les domaines locaux du conteneur : `service.projet.solon.local` pour un service Compose,
    /// `nom.solon.local` toujours.
    pub fn domains(&self) -> Vec<String> {
        let mut d = Vec::new();
        if let (Some(p), Some(s)) = (&self.project, &self.service) {
            d.push(format!("{s}.{p}.solon.local"));
        }
        d.push(format!("{}.solon.local", self.name));
        d
    }
}

/// Tous les conteneurs (`docker ps -a`).
pub async fn list() -> Result<Vec<Container>, String> {
    let (code, out, err) = exec(
        "docker ps -a --no-trunc --format '{{.ID}}|{{.Names}}|{{.Label \"com.docker.compose.project\"}}|{{.Label \"com.docker.compose.service\"}}|{{.State}}|{{.Status}}|{{.Ports}}'".into(),
        30,
    )
    .await?;
    if code != 0 {
        return Err(format!("docker ps: {}", err.trim()));
    }
    Ok(parse(&out))
}

fn parse(out: &str) -> Vec<Container> {
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(7, '|').collect();
            (f.len() == 7).then(|| Container {
                id: f[0].to_owned(),
                name: f[1].to_owned(),
                project: opt(f[2]),
                service: opt(f[3]),
                state: f[4].to_owned(),
                status: f[5].to_owned(),
                ports: f[6].to_owned(),
            })
        })
        .collect()
}

/// `docker start|stop|restart` sur un conteneur.
pub async fn act(action: &str, id: &str) -> Result<(), String> {
    if !matches!(action, "start" | "stop" | "restart") {
        return Err(format!("unknown action: {action}"));
    }
    let (code, _, err) = exec(
        format!("docker {action} {}", crate::control::shell_quote(id)),
        120,
    )
    .await?;
    if code != 0 {
        return Err(format!("docker {action}: {}", err.trim()));
    }
    Ok(())
}

/// `docker compose -p <projet> start|stop|restart` : Compose retrouve le projet par ses étiquettes,
/// sans le dossier Windows.
pub async fn project_act(action: &str, project: &str) -> Result<(), String> {
    if !matches!(action, "start" | "stop" | "restart") {
        return Err(format!("unknown action: {action}"));
    }
    let (code, _, err) = exec(
        format!(
            "docker compose -p {} {action}",
            crate::control::shell_quote(project)
        ),
        300,
    )
    .await?;
    if code != 0 {
        return Err(format!("docker compose {action}: {}", err.trim()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lecture_de_docker_ps() {
        let l = parse(
            "abc|blog-web-1|blog|web|running|Up 2 hours|0.0.0.0:8080->80/tcp\nxyz|lonely||exited|Exited (0)|\n",
        );
        assert_eq!(l.len(), 1, "une ligne à six champs est ignorée");
        assert_eq!(
            l[0].domains(),
            vec!["web.blog.solon.local", "blog-web-1.solon.local"]
        );
        let l = parse("xyz|lonely|||exited|Exited (0)|\n");
        assert_eq!(l[0].project, None);
        assert!(!l[0].running());
        assert_eq!(l[0].domains(), vec!["lonely.solon.local"]);
    }
}
