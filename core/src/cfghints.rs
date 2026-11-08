//! Подсказки к настройкам ядра: что делает параметр и какое значение разумно.
//! Ключ — имя файла и путь параметра. Для spigot/purpur секция мира
//! `world-settings.<мир>` ищется как `world-settings.default`.

/// (файл, путь, по-русски, по-английски)
type H = (&'static str, &'static str, &'static str, &'static str);

const HINTS: &[H] = &[
    // ---------------------------------------------------------------- server.properties
    ("server.properties", "motd", "Описание сервера в списке серверов игрока. Цвета — коды §, например §aзелёный.", "Server description shown in the player's server list. Colours via § codes, e.g. §a for green."),
    ("server.properties", "server-port", "Порт сервера. По умолчанию 25565 — тогда друзьям не нужно указывать порт в адресе. Если меняешь, открой новый порт в роутере.", "Server port. Default 25565 — then friends don't have to type the port. If you change it, forward the new port on your router."),
    ("server.properties", "server-ip", "Оставь пустым. Указывается, только если у ПК несколько сетевых адресов и сервер должен слушать один из них.", "Leave empty. Only set it if the PC has several addresses and the server must listen on one."),
    ("server.properties", "max-players", "Сколько игроков может быть одновременно.", "Maximum number of players online at once."),
    ("server.properties", "online-mode", "Проверка лицензии через Mojang. true — только лицензионные аккаунты, ники нельзя подделать. false — пускает всех (пиратки), но любой может зайти под чужим ником: обязательно ставь плагин/мод авторизации.", "Account verification via Mojang. true — only legit accounts, names can't be spoofed. false — anyone can join (cracked), but anyone can use any name: use an auth plugin/mod."),
    ("server.properties", "white-list", "Белый список: заходят только добавленные игроки (вкладка «Игроки»).", "Whitelist: only listed players can join (Players tab)."),
    ("server.properties", "enforce-whitelist", "Если включить белый список на ходу — сразу кикнуть тех, кого в нём нет.", "When the whitelist is turned on, immediately kick players who are not on it."),
    ("server.properties", "pvp", "Могут ли игроки бить друг друга.", "Whether players can damage each other."),
    ("server.properties", "difficulty", "Сложность: peaceful (без монстров), easy, normal, hard.", "Difficulty: peaceful (no monsters), easy, normal, hard."),
    ("server.properties", "hardcore", "Хардкор: после смерти игрок становится наблюдателем. Сложность всегда hard.", "Hardcore: players become spectators after death. Difficulty is locked to hard."),
    ("server.properties", "gamemode", "Режим игры для новых игроков: survival, creative, adventure, spectator.", "Game mode for new players: survival, creative, adventure, spectator."),
    ("server.properties", "force-gamemode", "При каждом входе ставить игроку режим из gamemode.", "Reset players to the default game mode on every join."),
    ("server.properties", "view-distance", "Дальность прорисовки в чанках, которую сервер отправляет игрокам. Сильно влияет на нагрузку и трафик. 8–10 — хороший баланс.", "How many chunks the server sends to players. Big impact on CPU and bandwidth. 8–10 is a good balance."),
    ("server.properties", "simulation-distance", "В каком радиусе (чанков) вокруг игрока работают мобы, фермы, рост растений. Меньше — легче серверу. 5–6 обычно хватает.", "Radius (chunks) around players where mobs, farms and crops tick. Lower is lighter. 5–6 is usually enough."),
    ("server.properties", "spawn-protection", "Радиус вокруг спавна, где строить могут только операторы. 0 — выключено.", "Radius around spawn where only operators can build. 0 disables it."),
    ("server.properties", "level-name", "Папка мира. Поменяешь — сервер создаст/загрузит другой мир.", "World folder. Changing it loads or creates another world."),
    ("server.properties", "level-seed", "Сид для генерации нового мира. На уже созданный мир не влияет.", "Seed for generating a new world. Doesn't affect an existing world."),
    ("server.properties", "level-type", "Тип нового мира: minecraft:normal, minecraft:flat, minecraft:large_biomes, minecraft:amplified.", "Type of a new world: minecraft:normal, minecraft:flat, minecraft:large_biomes, minecraft:amplified."),
    ("server.properties", "allow-nether", "Разрешить Незер.", "Allow the Nether."),
    ("server.properties", "allow-flight", "Не кикать за «полёт». Включи, если на сервере есть моды/плагины с полётом (джетпаки, крылья) — иначе игроков будет выкидывать.", "Don't kick for flying. Turn on if mods/plugins allow flight (jetpacks, wings), otherwise players get kicked."),
    ("server.properties", "spawn-monsters", "Появляются ли враждебные мобы.", "Whether hostile mobs spawn."),
    ("server.properties", "spawn-animals", "Появляются ли животные.", "Whether animals spawn."),
    ("server.properties", "spawn-npcs", "Появляются ли жители.", "Whether villagers spawn."),
    ("server.properties", "generate-structures", "Генерировать деревни, крепости и другие постройки в новых чанках.", "Generate villages, strongholds and other structures in new chunks."),
    ("server.properties", "max-world-size", "Максимальный радиус мира в блоках (граница мира).", "Maximum world radius in blocks (world border)."),
    ("server.properties", "network-compression-threshold", "Пакеты больше этого размера (байт) сжимаются. 256 — стандарт. Для медленного интернета у игроков можно 128, для локальной сети — 512 или -1 (без сжатия).", "Packets larger than this (bytes) are compressed. 256 is standard. Slow player connections: 128; LAN: 512 or -1 (off)."),
    ("server.properties", "max-tick-time", "Если один тик длится дольше (мс), сервер считает себя зависшим и останавливается. -1 — никогда. На больших сборках при загрузке мира лучше -1 или 120000+.", "If one tick takes longer (ms) the watchdog stops the server. -1 disables. Large modpacks: -1 or 120000+."),
    ("server.properties", "enable-command-block", "Разрешить командные блоки.", "Allow command blocks."),
    ("server.properties", "enable-rcon", "Удалённая консоль RCON. Включай, только если нужна, и задай сложный rcon.password; порт RCON не открывай в интернет.", "RCON remote console. Only enable if needed, set a strong rcon.password and never expose the RCON port to the internet."),
    ("server.properties", "rcon.password", "Пароль RCON. Должен быть длинным и случайным.", "RCON password. Make it long and random."),
    ("server.properties", "enable-query", "Протокол Query для мониторингов серверов (список игроков, версия).", "Query protocol for server listing sites (player list, version)."),
    ("server.properties", "op-permission-level", "Уровень прав операторов: 1 — обход защиты спавна, 2 — читы и командные блоки, 3 — кик/бан, 4 — всё, включая /stop.", "Operator permission level: 1 bypass spawn protection, 2 cheats & command blocks, 3 kick/ban, 4 everything incl. /stop."),
    ("server.properties", "player-idle-timeout", "Кикать игрока после стольких минут бездействия. 0 — не кикать.", "Kick players idle for this many minutes. 0 disables."),
    ("server.properties", "resource-pack", "Ссылка на ресурспак, который предлагается игрокам при входе.", "URL of a resource pack offered to players on join."),
    ("server.properties", "require-resource-pack", "Кикать игроков, отказавшихся от ресурспака.", "Kick players who decline the resource pack."),
    ("server.properties", "enforce-secure-profile", "Требовать подписанный чат (с 1.19). Если игроки с пиратскими клиентами не могут писать в чат — поставь false.", "Require signed chat (1.19+). If cracked clients can't chat, set false."),
    ("server.properties", "sync-chunk-writes", "Синхронная запись чанков: надёжнее при сбое питания, но медленнее. На SSD можно оставить true.", "Synchronous chunk writes: safer on power loss but slower. Fine to keep true on an SSD."),
    ("server.properties", "entity-broadcast-range-percentage", "Насколько далеко игроки видят сущности (в % от стандарта). 50–80 снижает нагрузку на клиентов.", "How far players see entities (percent of default). 50–80 lowers client load."),
    ("server.properties", "rate-limit", "Максимум пакетов в секунду от игрока, больше — кик. 0 — без лимита.", "Max packets per second from a player before a kick. 0 disables."),
    ("server.properties", "hide-online-players", "Не показывать список игроков в списке серверов.", "Hide the player list in the server list."),
    ("server.properties", "accepts-transfers", "Разрешить переход игроков с других серверов командой /transfer.", "Allow players to be transferred in from other servers via /transfer."),
    // ---------------------------------------------------------------- bukkit.yml
    ("bukkit.yml", "settings.allow-end", "Разрешить Энд. false — портал в Энд не работает.", "Allow the End. false disables End portals."),
    ("bukkit.yml", "settings.warn-on-overload", "Писать в консоль «Can't keep up!», когда сервер не успевает.", "Log \"Can't keep up!\" when the server falls behind."),
    ("bukkit.yml", "settings.connection-throttle", "Минимальная пауза (мс) между подключениями с одного IP — защита от спама входами. -1 — выключено; при BungeeCord/Velocity ставь -1.", "Minimum delay (ms) between connections from one IP. -1 disables; set -1 behind BungeeCord/Velocity."),
    ("bukkit.yml", "settings.shutdown-message", "Сообщение игрокам при выключении сервера.", "Message shown to players when the server shuts down."),
    ("bukkit.yml", "spawn-limits.monsters", "Сколько враждебных мобов может быть вокруг одного игрока. Стандарт 70; 40–50 заметно разгружает сервер.", "Max hostile mobs per player area. Default 70; 40–50 noticeably lowers load."),
    ("bukkit.yml", "spawn-limits.animals", "Лимит животных. Стандарт 10.", "Animal cap. Default 10."),
    ("bukkit.yml", "spawn-limits.water-animals", "Лимит водных животных (кальмары, дельфины). Стандарт 5.", "Water animal cap (squid, dolphins). Default 5."),
    ("bukkit.yml", "spawn-limits.water-ambient", "Лимит рыб. Стандарт 20; рыбы бесполезны для игры — можно 2–5.", "Fish cap. Default 20; fish are mostly decorative — 2–5 is fine."),
    ("bukkit.yml", "spawn-limits.water-underground-creature", "Лимит светящихся кальмаров в подводных пещерах. Стандарт 5.", "Glow squid cap in underwater caves. Default 5."),
    ("bukkit.yml", "spawn-limits.axolotls", "Лимит аксолотлей. Стандарт 5.", "Axolotl cap. Default 5."),
    ("bukkit.yml", "spawn-limits.ambient", "Лимит летучих мышей. Стандарт 15; можно 1.", "Bat cap. Default 15; 1 is fine."),
    ("bukkit.yml", "ticks-per.monster-spawns", "Как часто (в тиках) пытаться спавнить монстров. 1 — каждый тик; 2–4 снижает нагрузку почти без разницы для игроков.", "How often (ticks) to try spawning monsters. 1 = every tick; 2–4 reduces load with little visible difference."),
    ("bukkit.yml", "ticks-per.animal-spawns", "Как часто (в тиках) пытаться спавнить животных. Стандарт 400.", "How often (ticks) to try spawning animals. Default 400."),
    ("bukkit.yml", "ticks-per.water-spawns", "Как часто спавнить водных животных (тики).", "How often to spawn water animals (ticks)."),
    ("bukkit.yml", "ticks-per.water-ambient-spawns", "Как часто спавнить рыб (тики). Можно увеличить.", "How often to spawn fish (ticks). Can be raised."),
    ("bukkit.yml", "ticks-per.ambient-spawns", "Как часто спавнить летучих мышей (тики).", "How often to spawn bats (ticks)."),
    ("bukkit.yml", "ticks-per.autosave", "Автосохранение мира раз в столько тиков (20 тиков = 1 с). На Paper автосохранение настраивается в paper-world-defaults.yml.", "Autosave every N ticks (20 ticks = 1 s). On Paper autosave is configured in paper-world-defaults.yml."),
    ("bukkit.yml", "chunk-gc.period-in-ticks", "Как часто выгружать ненужные чанки из памяти (тики).", "How often to unload unused chunks from memory (ticks)."),
    // ---------------------------------------------------------------- spigot.yml
    ("spigot.yml", "settings.bungeecord", "Включи, если сервер стоит за прокси BungeeCord/Waterfall. Тогда в server.properties поставь online-mode=false и закрой прямой доступ к порту.", "Enable when behind a BungeeCord/Waterfall proxy. Then set online-mode=false and block direct access to the port."),
    ("spigot.yml", "settings.restart-on-crash", "Запускать restart-script при падении. В менеджере есть свой автоперезапуск — лучше выключить, чтобы не было двух перезапусков.", "Run restart-script on crash. The manager has its own auto-restart — better turn this off."),
    ("spigot.yml", "settings.restart-script", "Скрипт для /restart и перезапуска при падении.", "Script used by /restart and crash restarts."),
    ("spigot.yml", "settings.timeout-time", "Через сколько секунд без ответа сервер считается зависшим (watchdog) и выключается. На больших сборках увеличь до 120–300.", "Seconds without response before the watchdog kills the server. Raise to 120–300 for big packs."),
    ("spigot.yml", "settings.save-user-cache-on-stop-only", "Сохранять usercache.json только при выключении — меньше записи на диск.", "Save usercache.json only on shutdown — fewer disk writes."),
    ("spigot.yml", "settings.moved-wrongly-threshold", "Порог античита на «неправильное» движение. Не трогай без причины.", "Anticheat threshold for wrong movement. Leave as is unless needed."),
    ("spigot.yml", "settings.moved-too-quickly-multiplier", "Порог античита на слишком быстрое движение. Если с модами на скорость игроков откатывает — увеличь.", "Anticheat threshold for moving too fast. Raise if speed effects rubber-band players."),
    ("spigot.yml", "settings.netty-threads", "Потоки для сети. 4 хватает почти всегда.", "Network threads. 4 is enough almost always."),
    ("spigot.yml", "settings.log-villager-deaths", "Писать в консоль о смерти жителей.", "Log villager deaths to the console."),
    ("spigot.yml", "settings.log-named-deaths", "Писать в консоль о смерти мобов с именем.", "Log deaths of named mobs."),
    ("spigot.yml", "settings.attribute.maxHealth.max", "Максимально возможное здоровье. Увеличь, если плагины/моды дают больше.", "Maximum possible health. Raise if plugins/mods grant more."),
    ("spigot.yml", "commands.tab-complete", "Автодополнение команд по Tab: 0 — все команды, -1 — выключено, N — после N символов.", "Tab completion: 0 all commands, -1 off, N after N characters."),
    ("spigot.yml", "commands.send-namespaced", "Показывать команды с префиксом (minecraft:give). false — короче список подсказок.", "Show namespaced commands (minecraft:give). false keeps suggestions shorter."),
    ("spigot.yml", "commands.log", "Писать в консоль команды игроков. Полезно для разборов.", "Log player commands to the console. Useful for investigations."),
    ("spigot.yml", "commands.silent-commandblock-console", "Не засорять консоль выводом командных блоков.", "Don't spam the console with command block output."),
    ("spigot.yml", "messages.whitelist", "Сообщение для тех, кого нет в белом списке.", "Message for players not on the whitelist."),
    ("spigot.yml", "messages.unknown-command", "Ответ на неизвестную команду.", "Reply to an unknown command."),
    ("spigot.yml", "messages.server-full", "Сообщение, когда сервер заполнен.", "Message when the server is full."),
    ("spigot.yml", "messages.outdated-client", "Сообщение для игроков со старой версией игры. {0} — версия сервера.", "Message for outdated clients. {0} is the server version."),
    ("spigot.yml", "messages.outdated-server", "Сообщение для игроков с более новой версией игры.", "Message for clients newer than the server."),
    ("spigot.yml", "messages.restart", "Сообщение при перезапуске.", "Message on restart."),
    ("spigot.yml", "world-settings.default.view-distance", "Дальность прорисовки для этого мира. default — как в server.properties.", "View distance for this world. default — use server.properties."),
    ("spigot.yml", "world-settings.default.simulation-distance", "Дальность симуляции для этого мира. default — как в server.properties.", "Simulation distance for this world. default — use server.properties."),
    ("spigot.yml", "world-settings.default.mob-spawn-range", "Радиус спавна мобов вокруг игрока (чанки). Меньше — мобы ближе и их меньше «теряется». Обычно 6, не больше simulation-distance.", "Mob spawn radius around players (chunks). Usually 6, not more than simulation-distance."),
    ("spigot.yml", "world-settings.default.entity-activation-range.animals", "Животные дальше этого расстояния (блоков) от игрока «засыпают» — почти не тратят ресурсы. Меньше — легче серверу, но фермы дальше не работают.", "Animals farther than this (blocks) from players go mostly idle. Lower is lighter but farms beyond stop working."),
    ("spigot.yml", "world-settings.default.entity-activation-range.monsters", "То же для монстров (блоки).", "Same for monsters (blocks)."),
    ("spigot.yml", "world-settings.default.entity-activation-range.raiders", "То же для рейдеров (блоки).", "Same for raiders (blocks)."),
    ("spigot.yml", "world-settings.default.entity-activation-range.misc", "То же для прочих сущностей: предметы, вагонетки (блоки).", "Same for misc entities: items, minecarts (blocks)."),
    ("spigot.yml", "world-settings.default.entity-activation-range.water", "То же для водных мобов (блоки).", "Same for water mobs (blocks)."),
    ("spigot.yml", "world-settings.default.entity-activation-range.villagers", "То же для жителей (блоки). Слишком мало — ломаются фермы жителей.", "Same for villagers (blocks). Too low breaks villager farms."),
    ("spigot.yml", "world-settings.default.entity-activation-range.flying-monsters", "То же для летающих монстров (блоки).", "Same for flying monsters (blocks)."),
    ("spigot.yml", "world-settings.default.entity-tracking-range.players", "На каком расстоянии (блоков) игроки видят других игроков.", "Distance (blocks) at which players see other players."),
    ("spigot.yml", "world-settings.default.entity-tracking-range.animals", "На каком расстоянии видны животные (блоки).", "Distance at which animals are visible (blocks)."),
    ("spigot.yml", "world-settings.default.entity-tracking-range.monsters", "На каком расстоянии видны монстры (блоки).", "Distance at which monsters are visible (blocks)."),
    ("spigot.yml", "world-settings.default.entity-tracking-range.misc", "На каком расстоянии видны прочие сущности (блоки).", "Distance at which misc entities are visible (blocks)."),
    ("spigot.yml", "world-settings.default.merge-radius.item", "Радиус склеивания лежащих предметов в одну стопку (блоки). 2.5–3.5 сильно уменьшает число сущностей.", "Radius for merging dropped items into one stack (blocks). 2.5–3.5 cuts entity count a lot."),
    ("spigot.yml", "world-settings.default.merge-radius.exp", "Радиус склеивания сфер опыта (блоки).", "Radius for merging experience orbs (blocks)."),
    ("spigot.yml", "world-settings.default.item-despawn-rate", "Через сколько тиков исчезают выброшенные предметы. 6000 = 5 минут.", "Ticks before dropped items despawn. 6000 = 5 minutes."),
    ("spigot.yml", "world-settings.default.arrow-despawn-rate", "Через сколько тиков исчезают стрелы в блоках. 1200 = 1 минута.", "Ticks before arrows stuck in blocks despawn. 1200 = 1 minute."),
    ("spigot.yml", "world-settings.default.nerf-spawner-mobs", "Мобы из спавнеров без ИИ (стоят на месте). Сильно облегчает фермы, но они становятся «глупыми».", "Spawner mobs get no AI. Big performance win for farms, but they stand still."),
    ("spigot.yml", "world-settings.default.max-tnt-per-tick", "Сколько TNT может взорваться за тик. Защита от лагмашин.", "Max TNT exploding per tick. Protects against lag machines."),
    ("spigot.yml", "world-settings.default.hopper-amount", "Сколько предметов воронка переносит за раз.", "Items a hopper moves per transfer."),
    ("spigot.yml", "world-settings.default.ticks-per.hopper-transfer", "Пауза между переносами воронки (тики). Стандарт 8.", "Delay between hopper transfers (ticks). Default 8."),
    ("spigot.yml", "world-settings.default.ticks-per.hopper-check", "Как часто воронка ищет предметы (тики). 8 вместо 1 заметно снижает нагрузку от воронок.", "How often hoppers look for items (ticks). 8 instead of 1 reduces hopper load."),
    ("spigot.yml", "world-settings.default.verbose", "Подробно писать настройки мира при запуске. Можно выключить, чтобы не засорять консоль.", "Print world settings verbosely at startup. Turn off to keep the console clean."),
    ("spigot.yml", "world-settings.default.growth.cactus-modifier", "Скорость роста кактусов в %. 100 — как в ванилле.", "Cactus growth speed in %. 100 = vanilla."),
    ("spigot.yml", "world-settings.default.growth.cane-modifier", "Скорость роста тростника в %.", "Sugar cane growth speed in %."),
    ("spigot.yml", "world-settings.default.growth.wheat-modifier", "Скорость роста пшеницы в %.", "Wheat growth speed in %."),
    ("spigot.yml", "world-settings.default.growth.sapling-modifier", "Скорость роста саженцев в %.", "Sapling growth speed in %."),
    ("spigot.yml", "world-settings.default.growth.melon-modifier", "Скорость роста арбузов в %.", "Melon growth speed in %."),
    ("spigot.yml", "world-settings.default.growth.pumpkin-modifier", "Скорость роста тыкв в %.", "Pumpkin growth speed in %."),
    // ---------------------------------------------------------------- paper-global.yml
    ("paper-global.yml", "proxies.velocity.enabled", "Включи, если сервер стоит за прокси Velocity (и задай secret из velocity.toml). online-mode в server.properties тогда false.", "Enable when behind a Velocity proxy (and set the secret from velocity.toml). Then online-mode=false."),
    ("paper-global.yml", "proxies.velocity.online-mode", "Проверять лицензию через Velocity. Обычно true.", "Verify accounts via Velocity. Usually true."),
    ("paper-global.yml", "proxies.velocity.secret", "Секрет из forwarding.secret Velocity. Никому не показывай.", "Secret from Velocity's forwarding.secret. Keep it private."),
    ("paper-global.yml", "proxies.bungee-cord.online-mode", "Проверка лицензии при BungeeCord. Должно совпадать с online-mode прокси.", "Account verification behind BungeeCord. Must match the proxy's online-mode."),
    ("paper-global.yml", "chunk-loading-basic.player-max-chunk-send-rate", "Сколько чанков в секунду отправлять одному игроку. Меньше — меньше трафика у игроков со слабым интернетом. -1 — без лимита.", "Chunks per second sent to one player. Lower helps players on slow internet. -1 = unlimited."),
    ("paper-global.yml", "chunk-loading-basic.player-max-chunk-load-rate", "Сколько чанков в секунду загружать для одного игрока. -1 — без лимита.", "Chunks per second loaded for one player. -1 = unlimited."),
    ("paper-global.yml", "chunk-loading-basic.player-max-chunk-generate-rate", "Сколько новых чанков в секунду генерировать для одного игрока. Снизь, если исследование мира грузит сервер. -1 — без лимита.", "New chunks per second generated for one player. Lower it if exploration lags the server. -1 = unlimited."),
    ("paper-global.yml", "chunk-system.worker-threads", "Потоки для генерации и загрузки чанков. -1 — автоматически (половина ядер).", "Threads for chunk generation/loading. -1 = automatic (half the cores)."),
    ("paper-global.yml", "chunk-system.io-threads", "Потоки чтения и записи чанков. -1 — автоматически.", "Threads for chunk I/O. -1 = automatic."),
    ("paper-global.yml", "misc.max-joins-per-tick", "Сколько игроков может зайти за один тик. Защита от лагов при массовом входе после рестарта.", "Players allowed to join per tick. Prevents lag when everyone reconnects after a restart."),
    ("paper-global.yml", "watchdog.early-warning-delay", "Через сколько мс зависания писать предупреждение со стеком потоков — помогает найти причину лага.", "After how many ms of hang to log an early warning with thread dumps — helps find lag sources."),
    ("paper-global.yml", "watchdog.early-warning-every", "Как часто повторять предупреждение о зависании (мс).", "How often to repeat the hang warning (ms)."),
    ("paper-global.yml", "spam-limiter.tab-spam-limit", "Лимит запросов автодополнения от игрока — защита от спама Tab.", "Limit on tab-complete requests per player — anti-spam."),
    ("paper-global.yml", "unsupported-settings.allow-piston-duplication", "Разрешить дюп через поршни (TNT, ковры, рельсы). По умолчанию Paper это чинит; включи, если игрокам нужны ванильные дюп-фермы.", "Allow piston duplication (TNT, carpet, rails). Paper fixes it by default; enable if players need vanilla dupers."),
    ("paper-global.yml", "unsupported-settings.allow-headless-pistons", "Разрешить «безголовые» поршни (используются в некоторых технических постройках).", "Allow headless pistons (used in some technical builds)."),
    ("paper-global.yml", "unsupported-settings.allow-permanent-block-break-exploits", "Разрешить ломать бедрок и порталы эксплойтами.", "Allow exploits that break bedrock and portal frames."),
    ("paper-global.yml", "console.enable-brigadier-highlighting", "Подсветка команд в консоли сервера.", "Command highlighting in the server console."),
    ("paper-global.yml", "logging.deobfuscate-stacktraces", "Понятные имена в ошибках вместо обфусцированных. Полезно для разбора крашей.", "Readable names in stack traces. Useful for crash analysis."),
    ("paper-global.yml", "collisions.enable-player-collisions", "Толкаются ли игроки друг о друга.", "Whether players push each other."),
    ("paper-global.yml", "messages.no-permission", "Сообщение, когда у игрока нет прав на команду.", "Message when a player lacks permission for a command."),
    ("paper-global.yml", "item-validation.book-size.page-max", "Максимальный размер страницы книги (байт) — защита от книг-крашеров.", "Max book page size (bytes) — protects against crash books."),
    // ---------------------------------------------------------------- paper-world-defaults.yml
    ("paper-world-defaults.yml", "chunks.auto-save-interval", "Автосохранение чанков раз в столько тиков. default — как в bukkit.yml (6000 = 5 минут).", "Autosave chunks every N ticks. default — bukkit.yml value (6000 = 5 minutes)."),
    ("paper-world-defaults.yml", "chunks.max-auto-save-chunks-per-tick", "Сколько чанков сохранять за тик при автосохранении. Меньше — сохранение растягивается и не даёт лагов.", "Chunks saved per tick during autosave. Lower spreads saving out and avoids spikes."),
    ("paper-world-defaults.yml", "chunks.delay-chunk-unloads-by", "Сколько держать чанк загруженным после ухода игрока. Помогает, когда игроки ходят туда-сюда.", "How long chunks stay loaded after players leave. Helps when players move back and forth."),
    ("paper-world-defaults.yml", "chunks.prevent-moving-into-unloaded-chunks", "Не пускать игроков в ещё не загруженные чанки — меньше лагов при быстром полёте.", "Stop players from moving into unloaded chunks — fewer lag spikes when flying fast."),
    ("paper-world-defaults.yml", "entities.spawning.per-player-mob-spawns", "Лимит мобов считается на каждого игрока отдельно — мобы распределяются честнее и их не «съедает» одна ферма.", "Mob caps are per player — fairer distribution, one farm can't eat the whole cap."),
    ("paper-world-defaults.yml", "entities.spawning.despawn-ranges.monster.hard", "Монстры дальше этого расстояния (блоков) исчезают сразу.", "Monsters farther than this (blocks) despawn immediately."),
    ("paper-world-defaults.yml", "entities.spawning.despawn-ranges.monster.soft", "Монстры дальше этого расстояния (блоков) могут исчезать случайно.", "Monsters farther than this (blocks) may despawn randomly."),
    ("paper-world-defaults.yml", "entities.spawning.alt-item-despawn-rate.enabled", "Свои сроки исчезновения для отдельных предметов (например, булыжник быстрее).", "Custom despawn times for specific items (e.g. cobblestone faster)."),
    ("paper-world-defaults.yml", "entities.armor-stands.tick", "Обрабатывать стойки для брони каждый тик. false экономит ресурсы, если их много.", "Tick armor stands. false saves resources when there are many."),
    ("paper-world-defaults.yml", "entities.armor-stands.do-collision-entity-lookups", "Стойки для брони проверяют столкновения. false — легче серверу.", "Armor stands check collisions. false is lighter."),
    ("paper-world-defaults.yml", "environment.optimize-explosions", "Быстрый расчёт взрывов. Почти не отличается от ванильного, сильно снижает лаги от TNT.", "Faster explosion maths. Nearly identical to vanilla, much less TNT lag."),
    ("paper-world-defaults.yml", "environment.treasure-maps.enabled", "Карты сокровищ у картографов и в сундуках. Поиск структуры для карты может вызывать лаги.", "Treasure maps from cartographers and chests. Structure search for them can lag."),
    ("paper-world-defaults.yml", "environment.disable-ice-and-snow", "Не образуется лёд и снег — немного разгружает сервер.", "No ice and snow forming — slightly lighter."),
    ("paper-world-defaults.yml", "collisions.max-entity-collisions", "Сколько столкновений сущности за тик обрабатывать. 2–4 спасает от лагов при толпах мобов.", "Entity collisions processed per tick. 2–4 prevents lag from mob crowds."),
    ("paper-world-defaults.yml", "hopper.disable-move-event", "Не вызывать событие перемещения у воронок. Сильно ускоряет воронки, но плагины защиты сундуков могут перестать видеть их.", "Skip the hopper move event. Much faster hoppers, but chest-protection plugins may not notice them."),
    ("paper-world-defaults.yml", "hopper.ignore-occluding-blocks", "Воронки не ищут предметы сквозь сплошные блоки.", "Hoppers don't look for items through solid blocks."),
    ("paper-world-defaults.yml", "tick-rates.mob-spawner", "Как часто срабатывают спавнеры (тики). 2 — вдвое легче.", "How often spawners tick. 2 halves the load."),
    ("paper-world-defaults.yml", "tick-rates.grass-spread", "Как часто распространяется трава (тики). 4 почти незаметно и легче.", "How often grass spreads (ticks). 4 is barely noticeable and lighter."),
    ("paper-world-defaults.yml", "tick-rates.container-update", "Как часто обновляются открытые контейнеры (тики).", "How often open containers update (ticks)."),
    ("paper-world-defaults.yml", "misc.redstone-implementation", "Движок редстоуна: VANILLA, EIGENCRAFT или ALTERNATE_CURRENT. ALTERNATE_CURRENT намного быстрее, но редкие хитрые схемы могут вести себя иначе.", "Redstone engine: VANILLA, EIGENCRAFT or ALTERNATE_CURRENT. ALTERNATE_CURRENT is much faster; rare tricky circuits may behave differently."),
    ("paper-world-defaults.yml", "misc.update-pathfinding-on-block-update", "Пересчитывать пути мобов при каждом изменении блоков. false разгружает сервер, мобы чуть «тупее».", "Recalculate mob paths on every block change. false is lighter, mobs slightly dumber."),
    ("paper-world-defaults.yml", "anticheat.anti-xray.enabled", "Анти-иксрей: прячет руды от читеров. Немного нагружает сервер.", "Anti-xray: hides ores from cheaters. Adds a little load."),
    ("paper-world-defaults.yml", "anticheat.anti-xray.engine-mode", "Режим анти-иксрея: 1 — прячет руды за камнем, 2 — подмешивает фальшивые руды (надёжнее, больше трафика).", "Anti-xray mode: 1 hides ores as stone, 2 adds fake ores (stronger, more bandwidth)."),
    ("paper-world-defaults.yml", "fixes.disable-unloaded-chunk-enderpearl-exploit", "Чинит эксплойт с эндер-жемчугом в выгруженных чанках.", "Fixes the ender pearl exploit in unloaded chunks."),
    ("paper-world-defaults.yml", "fixes.fix-items-merging-through-walls", "Предметы не склеиваются сквозь стены.", "Items don't merge through walls."),
    // ---------------------------------------------------------------- purpur / pufferfish
    ("purpur.yml", "settings.use-alternate-keepalive", "Другой способ проверки связи с игроком — меньше киков «Timed out» у игроков с нестабильным интернетом.", "Alternate keepalive — fewer \"Timed out\" kicks for players with unstable internet."),
    ("purpur.yml", "world-settings.default.mobs.villager.lobotomize.enabled", "«Лоботомия» жителей, запертых в торговых залах: они не ищут пути и почти не грузят сервер.", "Lobotomize villagers stuck in trading halls: no pathfinding, almost no load."),
    ("pufferfish.yml", "dab.enabled", "Динамическая активация мозгов: дальние мобы думают реже. Заметно снижает нагрузку от мобов.", "Dynamic activation of brains: far mobs think less often. Noticeably reduces mob load."),
    ("pufferfish.yml", "enable-async-mob-spawning", "Расчёт спавна мобов в отдельном потоке.", "Compute mob spawning on a separate thread."),
    ("pufferfish.yml", "enable-suffocation-optimization", "Реже проверять удушье мобов в блоках — легче серверу.", "Check mob suffocation less often — lighter."),
];

fn file_key(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name {
        // настройки отдельного мира Paper — те же ключи, что и мир по умолчанию
        "paper-world.yml" => "paper-world-defaults.yml",
        "gale-world-defaults.yml" | "gale-world.yml" => "gale-world-defaults.yml",
        n => n,
    }
}

fn normalize(file: &str, path: &str) -> String {
    if (file == "spigot.yml" || file == "purpur.yml") && path.starts_with("world-settings.") {
        let mut parts: Vec<&str> = path.split('.').collect();
        if parts.len() > 2 {
            parts[1] = "default";
            return parts.join(".");
        }
    }
    path.to_string()
}

pub fn lookup(rel: &str, path: &str, en: bool) -> Option<String> {
    let file = file_key(rel);
    // server.properties ищем только в корне, остальные — по имени файла
    if file == "server.properties" && rel != "server.properties" {
        return None;
    }
    let p = normalize(file, path);
    HINTS.iter().find(|h| h.0 == file && h.1 == p).map(|h| if en { h.3 } else { h.2 }.to_string())
}

pub fn count() -> usize {
    HINTS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert!(lookup("server.properties", "view-distance", false).unwrap().contains("чанк"));
        assert!(lookup("spigot.yml", "world-settings.world_nether.entity-activation-range.animals", false).is_some());
        assert!(lookup("world/paper-world.yml", "environment.optimize-explosions", true).unwrap().contains("explosion"));
        assert!(lookup("plugins/X/server.properties", "motd", false).is_none());
        assert!(lookup("bukkit.yml", "nope", false).is_none());
        // нет дублей
        let mut seen = std::collections::HashSet::new();
        for h in HINTS {
            assert!(seen.insert((h.0, h.1)), "дубль {} {}", h.0, h.1);
            assert!(!h.2.is_empty() && !h.3.is_empty());
        }
        assert!(count() > 100);
    }
}
