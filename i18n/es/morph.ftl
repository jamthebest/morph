# Idioma
general-language = Language / Idioma
general-language-system = Sistema
general-language-en = English
general-language-es = Español
general-percent = { $n } %

# Páginas
page-music = Música
page-calendar = Calendario
page-todo = Tareas
page-shelf = Estante
page-desktops = Escritorios
page-dev = Desarrollo

# Fecha y hora
time-day-mon = lun
time-day-tue = mar
time-day-wed = mié
time-day-thu = jue
time-day-fri = vie
time-day-sat = sáb
time-day-sun = dom
time-month-jan = ene
time-month-feb = feb
time-month-mar = mar
time-month-apr = abr
time-month-may = may
time-month-jun = jun
time-month-jul = jul
time-month-aug = ago
time-month-sep = sep
time-month-oct = oct
time-month-nov = nov
time-month-dec = dic
time-12h-am = { $h }:{ $m } a. m.
time-12h-pm = { $h }:{ $m } p. m.
time-short-date = { $weekday } { $day } { $month }
cal-month-title = { $month } { $year }
time-pill = { $time } · { $weekday } { $day } { $month }

# Ajustes
settings-title = Ajustes
settings-section-general = General
settings-section-pages = Páginas
settings-section-shelf = Estante
settings-section-desktops = Escritorios
settings-section-dev = Desarrollo
settings-section-github = GitHub
settings-section-gitlab = GitLab
settings-section-terminal = Terminal
settings-section-notices = Avisos y HUD
settings-hour-12 = 12 h
settings-hour-24 = 24 h
settings-clock-format = Formato de hora
settings-size = Tamaño de la isla
settings-size-auto = Auto
settings-monitor = Monitor
settings-monitor-active = Activo
settings-open-on-hover = Abrir al pasar el mouse
settings-animations = Animaciones
settings-alarm-sounds = Sonidos de alarmas
settings-minimal-pill = Pastilla mínima
settings-hide-line = Ocultar en una línea
settings-panel-level = Al nivel del panel
settings-panel-level-hint = Se pone encima del panel y tapa lo que haya debajo
settings-pill-battery = Batería en la pastilla
settings-pill-volume = Volumen en la pastilla
settings-position = Posición
settings-center = Centrar
settings-autostart = Iniciar con la sesión
settings-hud = HUD
settings-notices = Avisos
settings-hud-style = Estilo
settings-hud-wide = Ancho
settings-hud-tall = Alto
settings-hud-volume = Al cambiar el volumen
settings-hud-brightness = Al cambiar el brillo
settings-notify-battery = Batería
settings-notify-wifi = Wi-Fi
settings-notify-bluetooth = Bluetooth
settings-notify-ci = Pipelines y Actions
settings-view = Vista
settings-shelf-grid = Cuadrícula
settings-shelf-list = Lista
settings-shelf-badge = Contador 📎 en la pastilla
settings-shelf-start-empty = Empezar vacío
settings-shelf-drag = Al arrastrar fuera
settings-shelf-move = Mover
settings-shelf-copy = Copiar
settings-desk-rows = Filas
settings-desk-columns = Columnas
settings-pinned-apps = APPS FIJADAS
settings-no-pins = Ninguna app fijada
settings-pin-label = { $app } · Escritorio { $n }
settings-pin-hint = Fija apps con clic derecho en la página Escritorios
settings-dev-columns = Columnas
settings-dev-tabs = Pestañas
settings-repos = Repositorios
settings-summary =
    { $repos ->
        [one] { $repos } repo de GitHub
       *[other] { $repos } repos de GitHub
    } · { $projects ->
        [one] { $projects } proyecto de GitLab
       *[other] { $projects } proyectos de GitLab
    }
settings-loading = Cargando…
settings-github-hint = Marca los repos que quieres ver en Desarrollo (se guarda al momento)
settings-add-manually = Agregar a mano
settings-github-placeholder = dueño/repo
settings-github-format = Formato: dueño/repo
settings-github-login = ¿Falta un repo? Inicia sesión con otra cuenta: gh auth login
settings-gitlab-server = Servidor de GitLab
settings-gitlab-invalid = Dirección no válida
settings-gitlab-token = Token
settings-gitlab-connected-as = Conectado como @{ $user }
settings-gitlab-connected = Conectado
settings-gitlab-forget-token = Olvidar token
settings-gitlab-connect-hint = Conecta GitLab con un token de solo lectura (read_api)
settings-gitlab-create-token = Crear token
settings-gitlab-paste-token = Pega el token
settings-save = Guardar
settings-add = Agregar
settings-gitlab-add-manually = Agregar proyecto a mano
settings-gitlab-placeholder = grupo/sub/proyecto
settings-gitlab-format = Formato: grupo/sub/proyecto
settings-term-size = Tamaño
settings-term-large = Grande
settings-term-medium = Mediana
settings-term-font = Letra

# HUD y vista asomada
hud-volume = Volumen
hud-brightness = Brillo
hud-muted = mudo
peek-muted = mudo

# Avisos
notice-alarm = Alarma
notice-task-start = Empieza ahora · { $time }
notice-task-end = Se acabó el tiempo
notice-ci-failed = { $what } falló · { $project }
notice-ci-ok = { $what } terminó bien · { $project }
notice-plug-in = Conecta el cargador
notice-battery-low = Batería baja · { $p } %
notice-battery-remaining = Quedan ~{ $m } min
notice-battery-critical = Batería crítica · { $p } %
notice-battery-full = Batería cargada
notice-battery-full-sub = 100 % · puedes desconectar
notice-wifi-connected = Wi-Fi conectado
notice-wifi-disconnected = Wi-Fi desconectado
notice-no-internet = Sin internet
notice-portal = La red pide iniciar sesión
notice-no-access = Conectado a { $ssid } sin acceso
notice-bt-connected = { $name } conectado
notice-bt-battery = Batería { $p } %
notice-bt-disconnected = { $name } desconectado
notice-missed-alarms-title = Alarmas perdidas
notice-missed-title = Avisos perdidos
notice-missed-alarms =
    { $n ->
        [one] { $n } alarma
       *[other] { $n } alarmas
    }
notice-missed-tasks =
    { $n ->
        [one] { $n } tarea
       *[other] { $n } tareas
    }
notice-missed-and = { $a } y { $b }
notice-missed-sub = { $parts } sin avisar
notice-view = Ver
notice-ok = OK
notice-done = Listo
notice-later = Más tarde
notice-snooze-15 = 15 min

# CLI
cli-page-needs-name = --page requiere un nombre: { $pages }
cli-unknown-page = página desconocida '{ $name }'; válidas: { $pages }
cli-unknown-arg = argumento desconocido '{ $arg }'; uso: morph [--toggle | --page <nombre> | --settings | --terminal]

# Calendario
cal-weekday-1 = lunes
cal-weekday-2 = martes
cal-weekday-3 = miércoles
cal-weekday-4 = jueves
cal-weekday-5 = viernes
cal-weekday-6 = sábado
cal-weekday-7 = domingo
cal-month-1 = Enero
cal-month-2 = Febrero
cal-month-3 = Marzo
cal-month-4 = Abril
cal-month-5 = Mayo
cal-month-6 = Junio
cal-month-7 = Julio
cal-month-8 = Agosto
cal-month-9 = Septiembre
cal-month-10 = Octubre
cal-month-11 = Noviembre
cal-month-12 = Diciembre
cal-month-of-1 = enero
cal-month-of-2 = febrero
cal-month-of-3 = marzo
cal-month-of-4 = abril
cal-month-of-5 = mayo
cal-month-of-6 = junio
cal-month-of-7 = julio
cal-month-of-8 = agosto
cal-month-of-9 = septiembre
cal-month-of-10 = octubre
cal-month-of-11 = noviembre
cal-month-of-12 = diciembre
cal-week-letter-1 = L
cal-week-letter-2 = M
cal-week-letter-3 = X
cal-week-letter-4 = J
cal-week-letter-5 = V
cal-week-letter-6 = S
cal-week-letter-7 = D
cal-long-date = { $weekday } { $day } de { $month }
cal-days-daily = Diario
cal-days-once = Una vez
cal-days-weekdays = L–V
cal-day-short-1 = L
cal-day-short-2 = M
cal-day-short-3 = X
cal-day-short-4 = J
cal-day-short-5 = V
cal-day-short-6 = S
cal-day-short-7 = D
cal-deleted = Borrado «{ $title }» ·
cal-undo = Deshacer
cal-snoozed-until = pospuesta hasta { $time }
cal-empty-day = Sin tareas ni alarmas
cal-add = + Agregar tarea o alarma
cal-notice-damaged = Agenda dañada
cal-save-failed = No se pudo guardar la agenda

# Tareas
todo-heading = To-do
todo-no-date = sin fecha
todo-today = hoy
todo-tomorrow = mañana
todo-empty = Sin tareas pendientes
todo-new-placeholder = Nueva tarea…
todo-add = Agregar

# Formulario
form-edit-task = Editar tarea
form-edit-alarm = Editar alarma
form-new-task = Nueva tarea
form-new-task-dated = Nueva tarea · { $date }
form-new-alarm = Nueva alarma
form-new-alarm-dated = Nueva alarma · { $date }
form-remove-time = Quitar hora
form-no-time = Sin hora
form-add-time = Agregar hora
form-kind-task = Tarea
form-kind-alarm = Alarma
form-title-placeholder = Título
form-duration-placeholder = Duración (min)
form-remind-start = al empezar
form-remind-end = al terminar
form-delete = Borrar
form-cancel = Cancelar
form-save = Guardar
form-am = a. m.
form-pm = p. m.
form-err-title = Escribe un título
form-err-time = Hora no válida
form-err-duration-needs-time = La duración necesita una hora
form-err-duration-range = Duración entre 1 y 1440 minutos
form-err-alarm-needs-time = La alarma necesita una hora
form-err-alarm-days = Marca al menos un día

# Música
music-idle = No se está reproduciendo nada
music-untitled = Reproduciendo
music-lyrics-loading = Buscando letra…
music-lyrics-unavailable = Letra no disponible
music-lyrics-instrumental = Instrumental
music-lyrics-credit = Letras: LRCLIB (lrclib.net)

# Shelf
shelf-title = Shelf
shelf-copy = Copiar
shelf-items =
    { $n ->
        [one] { $n } elemento
       *[other] { $n } elementos
    }
shelf-drag-all = Arrastrar todo
shelf-drag-all-copy = { $label } · Copiar
shelf-clear = Limpiar
shelf-empty-title = Suelta archivos o carpetas aquí
shelf-empty-hint = Luego arrástralos a cualquier app
shelf-drop-hint = Suelta para agregar al Shelf
shelf-decimal-sep = ,
shelf-notice-damaged = Shelf dañado
shelf-save-failed = No se pudo guardar el Shelf

# Persistencia (agenda y Shelf)
store-noun-agenda = la agenda
store-noun-shelf = el Shelf
store-read-failed = No se pudo leer { $what }; no se guardarán cambios
store-backup-saved = Se guardó una copia como { $name }
store-backup-failed-agenda = No se pudo respaldar la agenda dañada; no se guardarán cambios
store-backup-failed-shelf = No se pudo respaldar el Shelf dañado; no se guardarán cambios
store-create-failed = No se pudo crear { $path }: { $err }
store-write-failed = No se pudo escribir { $what }: { $err }
store-save-failed = No se pudo guardar { $what }: { $err }

# Desktops
desk-not-open = { $name } (sin abrir)
desk-pinned = { $name } fijada en el escritorio { $n }
desk-unpinned = { $name } ya no está fijada
desk-terminal-app = { $name } se abre en una terminal; no compatible todavía
desk-cannot-open = No se encontró cómo abrir { $app }

# Dev (shared)
dev-err-offline = sin conexión
dev-err-offline-ago = sin conexión · hace { $n } min
dev-ports-bad-pid = pid inválido
dev-ports-changed = el proceso ya no es el mismo
dev-no-access-all = Ningún repo tiene acceso · revisa + Repos / gh auth login
dev-no-access-some = { $n } sin acceso
dev-no-access = sin acceso
dev-sec-selected = Elegidos
dev-sec-local = En tu PC
dev-sec-accounts = Cuentas
dev-sec-account = Cuenta { $login }
dev-pr-draft = borrador
dev-pr-review = revisar tú
dev-pr-conflicts = conflictos
dev-pr-checks-failed = checks ✕
dev-pr-changes = cambios pedidos
dev-pr-changes-short = cambios
dev-pr-approved = aprobado
dev-pr-checks-pending = checks…
dev-pr-open = abierto
dev-stop-confirm-named = ¿Detener { $name } :{ $port }?
dev-stop-confirm = ¿Detener :{ $port }?
dev-run-running = en curso
dev-run-queued = en cola
dev-run-success = éxito
dev-run-failed = fallo
dev-run-cancelled = cancelada
dev-run-finished = terminada
dev-age-s = hace { $n } s
dev-age-min = hace { $n } min
dev-age-h = hace { $n } h
dev-yes = Sí
dev-no = No
dev-servers = Servidores
dev-no-servers = Sin servidores locales
dev-no-prs = Sin PRs abiertos
dev-no-runs = Sin ejecuciones
dev-pick-repos = Elige repos con + Repos
dev-rate-paused = Límite de la API · reintento luego
dev-loading = Cargando…
dev-tab-servers = Servidores · { $n }
dev-cannot-stop = No se puede detener ese proceso
dev-stop-failed = No se pudo detener el proceso

# GitHub
gh-err-login = Inicia sesión con `gh auth login`
gh-err-not-installed = gh no está instalado
gh-err-rate-limited = Límite de GitHub · reintento luego
gh-err-query = Algún repo no existe o no tienes acceso
gh-err-http = GitHub respondió { $code }

# GitLab
gl-keyring-label = Morph · GitLab ({ $host })
gl-err-no-token = Falta el token de GitLab
gl-err-keyring = No se pudo usar el llavero del sistema
gl-err-unauthorized = Token no válido
gl-err-scope = El token necesita permiso read_api
gl-err-rate-limited = Límite de GitLab · reintento luego
gl-err-http = GitLab respondió { $code }
gl-sec-selected = Elegidos (GitLab)
gl-sec-local = En tu PC (GitLab)
gl-sec-projects = Tus proyectos de GitLab
gl-needs-token = Conecta GitLab con un token para ver tus proyectos
gl-connected-as = Conectado como @{ $user }
gl-token-forgotten = Token olvidado

# Island
island-move-up = Suelta para mover arriba
island-move-down = Suelta para mover abajo
