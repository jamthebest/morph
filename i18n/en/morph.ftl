# Idioma
general-language = Language / Idioma
general-language-system = System
general-language-en = English
general-language-es = Español
general-percent = { $n }%

# Páginas
page-music = Music
page-calendar = Calendar
page-todo = Tasks
page-shelf = Shelf
page-desktops = Desktops
page-dev = Development

# Fecha y hora
time-day-mon = Mon
time-day-tue = Tue
time-day-wed = Wed
time-day-thu = Thu
time-day-fri = Fri
time-day-sat = Sat
time-day-sun = Sun
time-month-jan = Jan
time-month-feb = Feb
time-month-mar = Mar
time-month-apr = Apr
time-month-may = May
time-month-jun = Jun
time-month-jul = Jul
time-month-aug = Aug
time-month-sep = Sep
time-month-oct = Oct
time-month-nov = Nov
time-month-dec = Dec
time-12h-am = { $h }:{ $m } AM
time-12h-pm = { $h }:{ $m } PM
time-short-date = { $weekday } { $day } { $month }
cal-month-title = { $month } { $year }
time-pill = { $time } · { $weekday } { $day } { $month }

# Ajustes
settings-title = Settings
settings-section-general = General
settings-section-pages = Pages
settings-section-shelf = Shelf
settings-section-desktops = Desktops
settings-section-dev = Development
settings-section-github = GitHub
settings-section-gitlab = GitLab
settings-section-terminal = Terminal
settings-section-notices = Notices & HUD
settings-hour-12 = 12-hour
settings-hour-24 = 24-hour
settings-clock-format = Time format
settings-size = Island size
settings-size-auto = Auto
settings-monitor = Monitor
settings-monitor-active = Active
settings-open-on-hover = Open on hover
settings-animations = Animations
settings-alarm-sounds = Alarm sounds
settings-minimal-pill = Minimal pill
settings-hide-line = Hide as a line
settings-panel-level = At panel level
settings-panel-level-hint = Sits above the panel and covers whatever is below
settings-pill-battery = Battery in the pill
settings-pill-volume = Volume in the pill
settings-position = Position
settings-center = Center
settings-autostart = Start at login
settings-hud = HUD
settings-notices = Notices
settings-hud-style = Style
settings-hud-wide = Wide
settings-hud-tall = Tall
settings-hud-volume = When volume changes
settings-hud-brightness = When brightness changes
settings-notify-battery = Battery
settings-notify-wifi = Wi-Fi
settings-notify-bluetooth = Bluetooth
settings-notify-ci = Pipelines and Actions
settings-view = View
settings-shelf-grid = Grid
settings-shelf-list = List
settings-shelf-badge = 📎 counter in the pill
settings-shelf-start-empty = Start empty
settings-shelf-drag = When dragging out
settings-shelf-move = Move
settings-shelf-copy = Copy
settings-desk-rows = Rows
settings-desk-columns = Columns
settings-pinned-apps = PINNED APPS
settings-no-pins = No pinned apps
settings-pin-label = { $app } · Desktop { $n }
settings-pin-hint = Pin apps with a right click on the Desktops page
settings-dev-columns = Columns
settings-dev-tabs = Tabs
settings-repos = Repositories
settings-summary =
    { $repos ->
        [one] { $repos } GitHub repo
       *[other] { $repos } GitHub repos
    } · { $projects ->
        [one] { $projects } GitLab project
       *[other] { $projects } GitLab projects
    }
settings-loading = Loading…
settings-github-hint = Check the repos you want to see in Development (saved instantly)
settings-add-manually = Add manually
settings-github-placeholder = owner/repo
settings-github-format = Format: owner/repo
settings-github-login = Missing a repo? Sign in with another account: gh auth login
settings-gitlab-server = GitLab server
settings-gitlab-invalid = Invalid address
settings-gitlab-token = Token
settings-gitlab-connected-as = Connected as @{ $user }
settings-gitlab-connected = Connected
settings-gitlab-forget-token = Forget token
settings-gitlab-connect-hint = Connect GitLab with a read-only token (read_api)
settings-gitlab-create-token = Create token
settings-gitlab-paste-token = Paste the token
settings-save = Save
settings-add = Add
settings-gitlab-add-manually = Add project manually
settings-gitlab-placeholder = group/sub/project
settings-gitlab-format = Format: group/sub/project
settings-term-size = Size
settings-term-large = Large
settings-term-medium = Medium
settings-term-font = Font

# HUD y vista asomada
hud-volume = Volume
hud-brightness = Brightness
hud-muted = muted
peek-muted = muted

# Avisos
notice-alarm = Alarm
notice-task-start = Starting now · { $time }
notice-task-end = Time's up
notice-ci-failed = { $what } failed · { $project }
notice-ci-ok = { $what } succeeded · { $project }
notice-plug-in = Plug in the charger
notice-battery-low = Battery low · { $p }%
notice-battery-remaining = About { $m } min left
notice-battery-critical = Battery critical · { $p }%
notice-battery-full = Battery charged
notice-battery-full-sub = 100% · you can unplug
notice-wifi-connected = Wi-Fi connected
notice-wifi-disconnected = Wi-Fi disconnected
notice-no-internet = No internet
notice-portal = The network requires sign-in
notice-no-access = Connected to { $ssid } without access
notice-bt-connected = { $name } connected
notice-bt-battery = Battery { $p }%
notice-bt-disconnected = { $name } disconnected
notice-missed-alarms-title = Missed alarms
notice-missed-title = Missed notices
notice-missed-alarms =
    { $n ->
        [one] { $n } alarm
       *[other] { $n } alarms
    }
notice-missed-tasks =
    { $n ->
        [one] { $n } task
       *[other] { $n } tasks
    }
notice-missed-and = { $a } and { $b }
notice-missed-sub = { $parts } not notified
notice-view = View
notice-ok = OK
notice-done = Done
notice-later = Later
notice-snooze-15 = 15 min

# CLI
cli-page-needs-name = --page requires a name: { $pages }
cli-unknown-page = unknown page '{ $name }'; valid: { $pages }
cli-unknown-arg = unknown argument '{ $arg }'; usage: morph [--toggle | --page <name> | --settings | --terminal]

# Calendario
cal-weekday-1 = Monday
cal-weekday-2 = Tuesday
cal-weekday-3 = Wednesday
cal-weekday-4 = Thursday
cal-weekday-5 = Friday
cal-weekday-6 = Saturday
cal-weekday-7 = Sunday
cal-month-1 = January
cal-month-2 = February
cal-month-3 = March
cal-month-4 = April
cal-month-5 = May
cal-month-6 = June
cal-month-7 = July
cal-month-8 = August
cal-month-9 = September
cal-month-10 = October
cal-month-11 = November
cal-month-12 = December
cal-month-of-1 = January
cal-month-of-2 = February
cal-month-of-3 = March
cal-month-of-4 = April
cal-month-of-5 = May
cal-month-of-6 = June
cal-month-of-7 = July
cal-month-of-8 = August
cal-month-of-9 = September
cal-month-of-10 = October
cal-month-of-11 = November
cal-month-of-12 = December
cal-week-letter-1 = M
cal-week-letter-2 = T
cal-week-letter-3 = W
cal-week-letter-4 = T
cal-week-letter-5 = F
cal-week-letter-6 = S
cal-week-letter-7 = S
cal-long-date = { $weekday }, { $month } { $day }
cal-days-daily = Daily
cal-days-once = Once
cal-days-weekdays = Mon–Fri
cal-day-short-1 = Mo
cal-day-short-2 = Tu
cal-day-short-3 = We
cal-day-short-4 = Th
cal-day-short-5 = Fr
cal-day-short-6 = Sa
cal-day-short-7 = Su
cal-deleted = Deleted “{ $title }” ·
cal-undo = Undo
cal-snoozed-until = snoozed until { $time }
cal-empty-day = No tasks or alarms
cal-add = + Add task or alarm
cal-notice-damaged = Agenda damaged
cal-save-failed = Could not save the agenda

# Tareas
todo-heading = To-do
todo-no-date = no date
todo-today = today
todo-tomorrow = tomorrow
todo-empty = No pending tasks
todo-new-placeholder = New task…
todo-add = Add

# Formulario
form-edit-task = Edit task
form-edit-alarm = Edit alarm
form-new-task = New task
form-new-task-dated = New task · { $date }
form-new-alarm = New alarm
form-new-alarm-dated = New alarm · { $date }
form-remove-time = Remove time
form-no-time = No time
form-add-time = Add time
form-kind-task = Task
form-kind-alarm = Alarm
form-title-placeholder = Title
form-duration-placeholder = Duration (min)
form-remind-start = at start
form-remind-end = at end
form-delete = Delete
form-cancel = Cancel
form-save = Save
form-am = AM
form-pm = PM
form-err-title = Enter a title
form-err-time = Invalid time
form-err-duration-needs-time = Duration needs a time
form-err-duration-range = Duration must be 1 to 1440 minutes
form-err-alarm-needs-time = The alarm needs a time
form-err-alarm-days = Select at least one day

# Música
music-idle = Nothing is playing
music-untitled = Playing
music-lyrics-loading = Searching for lyrics…
music-lyrics-unavailable = Lyrics not available
music-lyrics-instrumental = Instrumental
music-lyrics-credit = Lyrics: LRCLIB (lrclib.net)

# Shelf
shelf-title = Shelf
shelf-copy = Copy
shelf-items =
    { $n ->
        [one] { $n } item
       *[other] { $n } items
    }
shelf-drag-all = Drag all
shelf-drag-all-copy = { $label } · Copy
shelf-clear = Clear
shelf-empty-title = Drop files or folders here
shelf-empty-hint = Then drag them to any app
shelf-drop-hint = Drop to add to the Shelf
shelf-decimal-sep = .
shelf-notice-damaged = Shelf damaged
shelf-save-failed = Could not save the Shelf

# Persistencia (agenda y Shelf)
store-noun-agenda = the agenda
store-noun-shelf = the Shelf
store-read-failed = Could not read { $what }; changes will not be saved
store-backup-saved = A copy was saved as { $name }
store-backup-failed-agenda = Could not back up the damaged agenda; changes will not be saved
store-backup-failed-shelf = Could not back up the damaged Shelf; changes will not be saved
store-create-failed = Could not create { $path }: { $err }
store-write-failed = Could not write { $what }: { $err }
store-save-failed = Could not save { $what }: { $err }

# Desktops
desk-not-open = { $name } (not open)
desk-pinned = { $name } pinned to desktop { $n }
desk-unpinned = { $name } is no longer pinned
desk-terminal-app = { $name } opens in a terminal; not supported yet
desk-cannot-open = Could not find how to open { $app }

# Dev (shared)
dev-err-offline = offline
dev-err-offline-ago = offline · { $n } min ago
dev-ports-bad-pid = invalid pid
dev-ports-changed = the process is no longer the same
dev-no-access-all = No repo is accessible · check + Repos / gh auth login
dev-no-access-some = { $n } without access
dev-no-access = no access
dev-sec-selected = Selected
dev-sec-local = On your PC
dev-sec-accounts = Accounts
dev-sec-account = Account { $login }
dev-pr-draft = draft
dev-pr-review = your review
dev-pr-conflicts = conflicts
dev-pr-checks-failed = checks ✕
dev-pr-changes = changes requested
dev-pr-changes-short = changes
dev-pr-approved = approved
dev-pr-checks-pending = checks…
dev-pr-open = open
dev-stop-confirm-named = Stop { $name } :{ $port }?
dev-stop-confirm = Stop :{ $port }?
dev-run-running = running
dev-run-queued = queued
dev-run-success = success
dev-run-failed = failed
dev-run-cancelled = cancelled
dev-run-finished = finished
dev-age-s = { $n } s ago
dev-age-min = { $n } min ago
dev-age-h = { $n } h ago
dev-yes = Yes
dev-no = No
dev-servers = Servers
dev-no-servers = No local servers
dev-no-prs = No open PRs
dev-no-runs = No runs
dev-pick-repos = Pick repos with + Repos
dev-rate-paused = API limit · retrying later
dev-loading = Loading…
dev-tab-servers = Servers · { $n }
dev-cannot-stop = Cannot stop that process
dev-stop-failed = Could not stop the process

# GitHub
gh-err-login = Sign in with `gh auth login`
gh-err-not-installed = gh is not installed
gh-err-rate-limited = GitHub limit · retrying later
gh-err-query = Some repo does not exist or you have no access
gh-err-http = GitHub responded { $code }

# GitLab
gl-keyring-label = Morph · GitLab ({ $host })
gl-err-no-token = GitLab token missing
gl-err-keyring = Could not use the system keyring
gl-err-unauthorized = Invalid token
gl-err-scope = The token needs the read_api scope
gl-err-rate-limited = GitLab limit · retrying later
gl-err-http = GitLab responded { $code }
gl-sec-selected = Selected (GitLab)
gl-sec-local = On your PC (GitLab)
gl-sec-projects = Your GitLab projects
gl-needs-token = Connect GitLab with a token to see your projects
gl-connected-as = Connected as @{ $user }
gl-token-forgotten = Token forgotten

# Island
island-move-up = Release to move to the top
island-move-down = Release to move to the bottom
