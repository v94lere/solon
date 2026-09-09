// Galerie de piles prêtes et suggestions d'environnement pour un dossier de projet.
// Les images officielles viennent du miroir public d'Amazon ECR (même contenu que Docker Hub, sans
// limite de débit agressive) ; les autres restent sur leur registre d'origine.
import type { Probe } from "./api";

export interface StackFile {
  path: string;
  content: string;
}

/** Texte dans les deux langues de l'interface. */
export type L = { en: string; fr: string };
export const tx = (l: L, lang: string) => (lang.startsWith("fr") ? l.fr : l.en);

export interface StackTemplate {
  id: string;
  name: string;
  /** Une phrase : à quoi ça sert. */
  tagline: L;
  tags: string[];
  /** Adresse à ouvrir une fois démarré (`{project}` est remplacé par le nom du dossier). */
  open?: string;
  files: StackFile[];
}

const LIB = "public.ecr.aws/docker/library";

const header = (title: string, hint: string) => `# ${title}\n# ${hint}\n`;

export const TEMPLATES: StackTemplate[] = [
  {
    id: "wordpress",
    name: "WordPress",
    tagline: { en: "WordPress site or blog with its MariaDB database.", fr: "Site ou blog WordPress avec sa base MariaDB." },
    tags: ["web", "php", "cms"],
    open: "http://wordpress.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("WordPress + MariaDB", "Open http://wordpress.<project>.solon.local/ or http://localhost:8080 and follow the installer.") +
          `services:
  db:
    image: ${LIB}/mariadb:11
    environment:
      MARIADB_DATABASE: wordpress
      MARIADB_USER: wordpress
      MARIADB_PASSWORD: wordpress
      MARIADB_ROOT_PASSWORD: root-demo
    volumes:
      - db:/var/lib/mysql
    healthcheck:
      test: ["CMD", "healthcheck.sh", "--connect", "--innodb_initialized"]
      interval: 5s
      timeout: 3s
      retries: 30

  wordpress:
    image: ${LIB}/wordpress:latest
    depends_on:
      db:
        condition: service_healthy
    ports:
      - "8080:80"
    environment:
      WORDPRESS_DB_HOST: db
      WORDPRESS_DB_USER: wordpress
      WORDPRESS_DB_PASSWORD: wordpress
      WORDPRESS_DB_NAME: wordpress
    volumes:
      - html:/var/www/html
      # Themes and plugins developed from Windows:
      # - ./wp-content:/var/www/html/wp-content

volumes:
  db:
  html:
`,
      },
    ],
  },
  {
    id: "odoo18",
    name: "Odoo 18",
    tagline: { en: "Odoo Community 18 ERP with PostgreSQL 16; your modules in ./addons.", fr: "ERP Odoo Community 18 avec PostgreSQL 16 ; vos modules dans ./addons." },
    tags: ["erp", "python", "postgres"],
    open: "http://odoo.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("Odoo 18 Community + PostgreSQL 16", "Open http://odoo.<project>.solon.local/ ; master password: see config/odoo.conf.") +
          `services:
  db:
    image: ${LIB}/postgres:16
    environment:
      POSTGRES_DB: postgres
      POSTGRES_USER: odoo
      POSTGRES_PASSWORD: odoo
    volumes:
      - db:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U odoo -d postgres"]
      interval: 5s
      timeout: 3s
      retries: 20

  odoo:
    image: ${LIB}/odoo:18.0
    depends_on:
      db:
        condition: service_healthy
    ports:
      - "8069:8069"
    environment:
      HOST: db
      USER: odoo
      PASSWORD: odoo
    volumes:
      - web:/var/lib/odoo
      - ./config:/etc/odoo
      - ./addons:/mnt/extra-addons

volumes:
  db:
  web:
`,
      },
      {
        path: "config/odoo.conf",
        content: `[options]
admin_passwd = admin-demo
addons_path = /usr/lib/python3/dist-packages/odoo/addons,/mnt/extra-addons
data_dir = /var/lib/odoo
`,
      },
      { path: "addons/README.md", content: "Put your Odoo modules (folders with `__manifest__.py`) here.\n" },
    ],
  },
  {
    id: "postgres",
    name: "PostgreSQL + Adminer",
    tagline: { en: "PostgreSQL 16 database with a web UI to browse it.", fr: "Base PostgreSQL 16 et une interface web pour la parcourir." },
    tags: ["database", "sql"],
    open: "http://adminer.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("PostgreSQL 16 + Adminer", "Adminer: http://adminer.<project>.solon.local/ (server: db, user: app, password: app). Port 5432 published for your Windows tools.") +
          `services:
  db:
    image: ${LIB}/postgres:16
    environment:
      POSTGRES_DB: app
      POSTGRES_USER: app
      POSTGRES_PASSWORD: app
    ports:
      - "5432:5432"
    volumes:
      - data:/var/lib/postgresql/data

  adminer:
    image: ${LIB}/adminer:latest
    depends_on: [db]
    environment:
      ADMINER_DEFAULT_SERVER: db

volumes:
  data:
`,
      },
    ],
  },
  {
    id: "mariadb",
    name: "MariaDB + Adminer",
    tagline: { en: "MariaDB 11 (MySQL-compatible) database with a web UI.", fr: "Base MariaDB 11 (compatible MySQL) et une interface web." },
    tags: ["database", "sql"],
    open: "http://adminer.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("MariaDB 11 + Adminer", "Adminer: http://adminer.<project>.solon.local/ (server: db, user: app, password: app). Port 3306 published.") +
          `services:
  db:
    image: ${LIB}/mariadb:11
    environment:
      MARIADB_DATABASE: app
      MARIADB_USER: app
      MARIADB_PASSWORD: app
      MARIADB_ROOT_PASSWORD: root-demo
    ports:
      - "3306:3306"
    volumes:
      - data:/var/lib/mysql

  adminer:
    image: ${LIB}/adminer:latest
    depends_on: [db]
    environment:
      ADMINER_DEFAULT_SERVER: db

volumes:
  data:
`,
      },
    ],
  },
  {
    id: "mongo",
    name: "MongoDB + Mongo Express",
    tagline: { en: "MongoDB 7 document database with its web explorer.", fr: "Base documentaire MongoDB 7 et son explorateur web." },
    tags: ["database", "nosql"],
    open: "http://mongo-express.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("MongoDB 7 + Mongo Express", "Explorer: http://mongo-express.<project>.solon.local/ (admin / admin). Port 27017 published.") +
          `services:
  mongo:
    image: ${LIB}/mongo:7
    environment:
      MONGO_INITDB_ROOT_USERNAME: root
      MONGO_INITDB_ROOT_PASSWORD: root-demo
    ports:
      - "27017:27017"
    volumes:
      - data:/data/db

  mongo-express:
    image: ${LIB}/mongo-express:latest
    depends_on: [mongo]
    environment:
      ME_CONFIG_MONGODB_URL: mongodb://root:root-demo@mongo:27017/
      ME_CONFIG_BASICAUTH_USERNAME: admin
      ME_CONFIG_BASICAUTH_PASSWORD: admin

volumes:
  data:
`,
      },
    ],
  },
  {
    id: "redis",
    name: "Redis",
    tagline: { en: "Redis 7 cache and message queue, port 6379 published.", fr: "Cache et file de messages Redis 7, port 6379 publié." },
    tags: ["cache"],
    files: [
      {
        path: "compose.yaml",
        content:
          header("Redis 7", "Port 6379 published; data kept in the volume.") +
          `services:
  redis:
    image: ${LIB}/redis:7
    command: ["redis-server", "--appendonly", "yes"]
    ports:
      - "6379:6379"
    volumes:
      - data:/data

volumes:
  data:
`,
      },
    ],
  },
  {
    id: "n8n",
    name: "n8n",
    tagline: { en: "Workflow automation (Zapier-like) with PostgreSQL.", fr: "Automatisation de flux (type Zapier) avec PostgreSQL." },
    tags: ["automation", "postgres"],
    open: "http://n8n.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("n8n + PostgreSQL", "Open http://n8n.<project>.solon.local/ and create the owner account.") +
          `services:
  db:
    image: ${LIB}/postgres:16
    environment:
      POSTGRES_DB: n8n
      POSTGRES_USER: n8n
      POSTGRES_PASSWORD: n8n
    volumes:
      - db:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U n8n -d n8n"]
      interval: 5s
      timeout: 3s
      retries: 20

  n8n:
    image: docker.n8n.io/n8nio/n8n:latest
    depends_on:
      db:
        condition: service_healthy
    ports:
      - "5678:5678"
    environment:
      DB_TYPE: postgresdb
      DB_POSTGRESDB_HOST: db
      DB_POSTGRESDB_DATABASE: n8n
      DB_POSTGRESDB_USER: n8n
      DB_POSTGRESDB_PASSWORD: n8n
      N8N_HOST: n8n.{project}.solon.local
      WEBHOOK_URL: http://n8n.{project}.solon.local/
      GENERIC_TIMEZONE: Europe/Luxembourg
    volumes:
      - n8n:/home/node/.n8n

volumes:
  db:
  n8n:
`,
      },
    ],
  },
  {
    id: "nextcloud",
    name: "Nextcloud",
    tagline: { en: "Personal cloud: files, calendar, contacts, with MariaDB and Redis.", fr: "Cloud personnel : fichiers, agenda, contacts, avec MariaDB et Redis." },
    tags: ["web", "php", "files"],
    open: "http://nextcloud.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("Nextcloud + MariaDB + Redis", "Open http://nextcloud.<project>.solon.local/ and create the admin account (database: db / nextcloud / nextcloud).") +
          `services:
  db:
    image: ${LIB}/mariadb:11
    command: --transaction-isolation=READ-COMMITTED --binlog-format=ROW
    environment:
      MARIADB_DATABASE: nextcloud
      MARIADB_USER: nextcloud
      MARIADB_PASSWORD: nextcloud
      MARIADB_ROOT_PASSWORD: root-demo
    volumes:
      - db:/var/lib/mysql

  redis:
    image: ${LIB}/redis:7

  nextcloud:
    image: ${LIB}/nextcloud:latest
    depends_on: [db, redis]
    ports:
      - "8081:80"
    environment:
      MYSQL_HOST: db
      MYSQL_DATABASE: nextcloud
      MYSQL_USER: nextcloud
      MYSQL_PASSWORD: nextcloud
      REDIS_HOST: redis
      NEXTCLOUD_TRUSTED_DOMAINS: nextcloud.{project}.solon.local localhost
    volumes:
      - html:/var/www/html

volumes:
  db:
  html:
`,
      },
    ],
  },
  {
    id: "ghost",
    name: "Ghost",
    tagline: { en: "Ghost 5 publishing platform with MySQL 8.", fr: "Plateforme de publication Ghost 5 avec MySQL 8." },
    tags: ["web", "cms", "node"],
    open: "http://ghost.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("Ghost 5 + MySQL 8", "Open http://ghost.<project>.solon.local/ghost/ to create the account.") +
          `services:
  db:
    image: ${LIB}/mysql:8
    environment:
      MYSQL_ROOT_PASSWORD: root-demo
      MYSQL_DATABASE: ghost
    volumes:
      - db:/var/lib/mysql

  ghost:
    image: ${LIB}/ghost:5
    depends_on: [db]
    ports:
      - "2368:2368"
    environment:
      url: http://ghost.{project}.solon.local
      database__client: mysql
      database__connection__host: db
      database__connection__user: root
      database__connection__password: root-demo
      database__connection__database: ghost
    volumes:
      - content:/var/lib/ghost/content

volumes:
  db:
  content:
`,
      },
    ],
  },
  {
    id: "gitea",
    name: "Gitea",
    tagline: { en: "Lightweight Git forge (repos, issues, CI) with PostgreSQL.", fr: "Forge Git légère (dépôts, tickets, CI) avec PostgreSQL." },
    tags: ["git", "postgres"],
    open: "http://gitea.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("Gitea + PostgreSQL", "Open http://gitea.<project>.solon.local/ ; SSH on port 2222.") +
          `services:
  db:
    image: ${LIB}/postgres:16
    environment:
      POSTGRES_DB: gitea
      POSTGRES_USER: gitea
      POSTGRES_PASSWORD: gitea
    volumes:
      - db:/var/lib/postgresql/data

  gitea:
    image: docker.io/gitea/gitea:latest
    depends_on: [db]
    ports:
      - "3000:3000"
      - "2222:22"
    environment:
      GITEA__database__DB_TYPE: postgres
      GITEA__database__HOST: db:5432
      GITEA__database__NAME: gitea
      GITEA__database__USER: gitea
      GITEA__database__PASSWD: gitea
    volumes:
      - data:/data

volumes:
  db:
  data:
`,
      },
    ],
  },
  {
    id: "uptime-kuma",
    name: "Uptime Kuma",
    tagline: { en: "Uptime monitoring for your sites and services.", fr: "Surveillance de disponibilité de vos sites et services." },
    tags: ["monitoring"],
    open: "http://uptime-kuma.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("Uptime Kuma", "Open http://uptime-kuma.<project>.solon.local/ and create the account.") +
          `services:
  uptime-kuma:
    image: docker.io/louislam/uptime-kuma:1
    ports:
      - "3001:3001"
    volumes:
      - data:/app/data

volumes:
  data:
`,
      },
    ],
  },
  {
    id: "static",
    name: "Static site (Nginx)",
    tagline: { en: "Serves the ./site folder with Nginx; ideal for an HTML mock-up.", fr: "Sert le dossier ./site avec Nginx ; idéal pour une maquette HTML." },
    tags: ["web", "html"],
    open: "http://web.{project}.solon.local/",
    files: [
      {
        path: "compose.yaml",
        content:
          header("Static Nginx site", "The ./site folder is served at http://web.<project>.solon.local/ ; edit the files, reload.") +
          `services:
  web:
    image: ${LIB}/nginx:alpine
    ports:
      - "8088:80"
    volumes:
      - ./site:/usr/share/nginx/html:ro
`,
      },
      {
        path: "site/index.html",
        content: `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>My site</title>
<body style="font-family: system-ui, sans-serif; max-width: 40em; margin: 4em auto">
<h1>It works.</h1>
<p>This page is served by Nginx in Solon from the <code>site/</code> folder. Edit it and reload.</p>
</body>
</html>
`,
      },
    ],
  },
];

// ---------------------------------------------------------------------------------------------
// Suggestions pour un dossier existant sans fichier Compose
// ---------------------------------------------------------------------------------------------

export interface StackSuggestion {
  id: string;
  title: L;
  summary: L;
  files: StackFile[];
  open?: string;
}

function devService(image: string, port: number, command: string, extraEnv = ""): string {
  return `services:
  app:
    image: ${image}
    working_dir: /app
    volumes:
      - .:/app
    ports:
      - "${port}:${port}"
    environment:
      PORT: "${port}"${extraEnv}
    command: ${command}
`;
}

/** Propose un ou plusieurs environnements d'après ce que la sonde a trouvé dans le dossier. */
export function suggestStacks(p: Probe): StackSuggestion[] {
  const out: StackSuggestion[] = [];
  const hdr = (title: string, hint: string) => header(title, hint);

  if (p.has_dockerfile) {
    out.push({
      id: "dockerfile",
      title: { en: "Build the folder's Dockerfile", fr: "Construire le Dockerfile du dossier" },
      summary: { en: "One service built from your Dockerfile, port 8080 published; adjust the port to your application.", fr: "Un service construit depuis votre Dockerfile, le port 8080 publié ; adaptez le port à votre application." },
      files: [
        {
          path: "compose.yaml",
          content:
            hdr("Application built from the Dockerfile", "Up builds the image; Rebuild rebuilds it after a change.") +
            `services:
  app:
    build: .
    ports:
      - "8080:8080"
`,
        },
      ],
    });
  }

  if (p.odoo_addons.length > 0) {
    const odoo = TEMPLATES.find((t) => t.id === "odoo18")!;
    const mods = `${p.odoo_addons.slice(0, 3).join(", ")}${p.odoo_addons.length > 3 ? "…" : ""}`;
    out.push({
      id: "odoo-addons",
      title: { en: `Odoo 18 with your modules (${mods})`, fr: `Odoo 18 avec vos modules (${mods})` },
      summary: { en: "The folder is mounted at /mnt/extra-addons: your modules show up in Apps after updating the list.", fr: "Le dossier est monté dans /mnt/extra-addons : vos modules apparaissent dans Applications après mise à jour de la liste." },
      open: odoo.open,
      files: [
        {
          path: "compose.yaml",
          content: odoo.files[0].content.replace("- ./addons:/mnt/extra-addons", "- .:/mnt/extra-addons"),
        },
        odoo.files[1],
      ],
    });
  }

  if (p.node_scripts.length > 0 || p.node_framework) {
    const script = p.node_scripts.includes("dev") ? "dev" : p.node_scripts.includes("start") ? "start" : p.node_scripts[0] ?? "start";
    const port = p.node_framework === "next" || p.node_framework === "nuxt" || p.node_framework === "express" || p.node_framework === "fastify" || p.node_framework === "nest" ? 3000 : p.node_framework === "vite" || p.node_framework === "vue" || p.node_framework === "svelte" || p.node_framework === "react" ? 5173 : p.node_framework === "angular" ? 4200 : 3000;
    out.push({
      id: "node",
      title: { en: `Node.js 22${p.node_framework ? ` (${p.node_framework})` : ""}`, fr: `Node.js 22${p.node_framework ? ` (${p.node_framework})` : ""}` },
      summary: { en: `Installs dependencies then runs "npm run ${script}" in the mounted folder; changes are picked up live.`, fr: `Installe les dépendances puis lance « npm run ${script} » dans le dossier monté ; les modifications sont vues en direct.` },
      files: [
        {
          path: "compose.yaml",
          content:
            hdr("Node.js application", `npm install then npm run ${script}; dev servers must listen on 0.0.0.0 (e.g. vite --host).`) +
            devService(`${LIB}/node:22`, port, `sh -c "npm install && npm run ${script} -- --host 0.0.0.0"`, "\n      HOST: 0.0.0.0"),
        },
      ],
    });
  }

  if (p.python_requirements || p.python_pyproject || p.python_entries.length > 0) {
    const entry = p.python_entries.includes("manage.py") ? "manage.py" : p.python_entries[0] ?? "main.py";
    const isDjango = entry === "manage.py";
    const run = isDjango ? "python manage.py runserver 0.0.0.0:8000" : `python ${entry}`;
    const install = p.python_requirements ? "pip install -r requirements.txt" : p.python_pyproject ? "pip install ." : "true";
    out.push({
      id: "python",
      title: { en: isDjango ? "Python 3.12 (Django)" : "Python 3.12", fr: isDjango ? "Python 3.12 (Django)" : "Python 3.12" },
      summary: { en: `${install}, then ${run}.`, fr: `${install}, puis ${run}.` },
      files: [
        {
          path: "compose.yaml",
          content:
            hdr("Python application", "The folder is mounted at /app; adapt the command to your entry point.") +
            devService(`${LIB}/python:3.12`, 8000, `sh -c "${install} && ${run}"`, "\n      PYTHONUNBUFFERED: \"1\""),
        },
      ],
    });
  }

  if (p.php_composer || p.php_files > 0) {
    out.push({
      id: "php",
      title: { en: "PHP 8.3 + Apache", fr: "PHP 8.3 + Apache" },
      summary: { en: "The folder is served by Apache; add a MariaDB database from the gallery if needed.", fr: "Le dossier est servi par Apache ; ajoutez une base MariaDB si besoin (galerie)." },
      files: [
        {
          path: "compose.yaml",
          content:
            hdr("PHP application", "Served at http://app.<project>.solon.local/ ; the folder is the site root.") +
            `services:
  app:
    image: ${LIB}/php:8.3-apache
    ports:
      - "8080:80"
    volumes:
      - .:/var/www/html
`,
        },
      ],
    });
  }

  if (p.go_mod) {
    out.push({
      id: "go",
      title: { en: "Go 1.23", fr: "Go 1.23" },
      summary: { en: "go run . in the mounted folder, port 8080.", fr: "go run . dans le dossier monté, port 8080." },
      files: [{ path: "compose.yaml", content: hdr("Go application", "go run . ; the module cache is kept in a volume.") + devService(`${LIB}/golang:1.23`, 8080, `sh -c "go run ."`) + "    volumes:\n      - .:/app\n      - gomod:/go/pkg/mod\n\nvolumes:\n  gomod:\n".replace("    volumes:\n      - .:/app\n", "") }],
    });
  }

  if (p.cargo) {
    out.push({
      id: "rust",
      title: { en: "Rust 1.85", fr: "Rust 1.85" },
      summary: { en: "cargo run in the mounted folder, port 8080.", fr: "cargo run dans le dossier monté, port 8080." },
      files: [{ path: "compose.yaml", content: hdr("Rust application", "cargo run ; the build target stays in a volume for speed.") + devService("docker.io/library/rust:1.85", 8080, `sh -c "cargo run"`, "\n      CARGO_TARGET_DIR: /target") + "\nvolumes:\n  target:\n" }],
    });
  }

  if (p.java_maven || p.java_gradle) {
    const cmd = p.java_maven ? "./mvnw spring-boot:run || mvn spring-boot:run" : "./gradlew bootRun";
    out.push({
      id: "java",
      title: { en: "Java 21", fr: "Java 21" },
      summary: { en: `${cmd} in the mounted folder, port 8080.`, fr: `${cmd} dans le dossier monté, port 8080.` },
      files: [{ path: "compose.yaml", content: hdr("Java application", "Adapt the command if this is not a Spring Boot project.") + devService(`${LIB}/eclipse-temurin:21`, 8080, `sh -c "${cmd}"`) }],
    });
  }

  if (p.dotnet_projects.length > 0) {
    out.push({
      id: "dotnet",
      title: { en: ".NET 8", fr: ".NET 8" },
      summary: { en: "dotnet watch run in the mounted folder, port 8080.", fr: "dotnet watch run dans le dossier monté, port 8080." },
      files: [{ path: "compose.yaml", content: hdr(".NET application", "dotnet watch run reloads on every change.") + devService("mcr.microsoft.com/dotnet/sdk:8.0", 8080, `sh -c "dotnet watch run --urls http://0.0.0.0:8080"`, "\n      DOTNET_USE_POLLING_FILE_WATCHER: \"1\"") }],
    });
  }

  if (p.ruby_gemfile) {
    out.push({
      id: "ruby",
      title: { en: "Ruby 3.3", fr: "Ruby 3.3" },
      summary: { en: "bundle install then bin/rails server (or adapt), port 3000.", fr: "bundle install puis bin/rails server (ou adaptez), port 3000." },
      files: [{ path: "compose.yaml", content: hdr("Ruby application", "Adapt the command if this is not Rails.") + devService(`${LIB}/ruby:3.3`, 3000, `sh -c "bundle install && bin/rails server -b 0.0.0.0"`) }],
    });
  }

  if (p.index_html && out.length === 0) {
    const t = TEMPLATES.find((x) => x.id === "static")!;
    out.push({
      id: "static",
      title: { en: "Static site (Nginx)", fr: "Site statique (Nginx)" },
      summary: { en: "The folder is served as is by Nginx.", fr: "Le dossier est servi tel quel par Nginx." },
      open: t.open,
      files: [{ path: "compose.yaml", content: t.files[0].content.replace("./site:", ".:") }],
    });
  }

  return out;
}

export function fillProject(text: string, project: string): string {
  return text.replaceAll("{project}", project);
}
