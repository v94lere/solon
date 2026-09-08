# Security policy

Monodon runs a Windows service as LocalSystem, a Linux virtual machine, a Docker engine, a local
HTTPS certificate authority and a file server for your Windows drives. Security reports are
taken seriously and handled in private first.

## Reporting a vulnerability

Please **do not** open a public issue for a security problem.

- Use GitHub's private vulnerability reporting on this repository
  (**Security → Report a vulnerability**), or
- e-mail the maintainer at the address listed on the GitHub profile of the repository owner,
  with "Monodon security" in the subject.

Include what you found, how to reproduce it, which version of Monodon and Windows you used, and
what impact you think it has. You will get an acknowledgement within 7 days and a status
update at least every 14 days until the report is resolved.

## What happens next

1. The report is confirmed and its severity assessed.
2. A fix is prepared in private and a new installer is built.
3. The fix is released with a security note in the release notes; the reporter is credited
   unless they prefer not to be.

## Scope

In scope: the Windows service, the desktop application, the guest agent and the engine image,
the installer, the `monodon.local` proxies and local certificate authority, monodonfs (the file
server for Windows drives), and the bundled `docker.exe` launcher.

Out of scope: vulnerabilities in the upstream Docker engine, containerd, runc, the Linux
kernel or Alpine packages (report them upstream; Monodon will pick up the fixed versions), and
issues that require an attacker to already be a local administrator.

## Supported versions

Only the latest release receives security fixes.
