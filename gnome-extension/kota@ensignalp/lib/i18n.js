// Kota — TR/EN metinler ve biçimlendirme.
// Hem eklenti (gnome-shell) hem tercihler (prefs) süreci kullanır; bu yüzden
// yalnızca GLib'e bağlıdır, Shell modüllerini içe aktarmaz.

import GLib from 'gi://GLib';

const TEXT = {
    tr: {
        app_tagline: 'AI kullanımı',
        refresh_now: 'Şimdi yenile',
        settings: 'Ayarlar',
        pause: 'Duraklat',
        resume: 'Sürdür',
        paused: 'Duraklatıldı',
        updated_at: 'Son güncelleme',
        loading: 'Yükleniyor…',
        no_windows: 'Limit bilgisi yok',
        no_providers: 'Etkin sağlayıcı yok — Ayarlar → Sağlayıcılar',
        service_missing: 'Kota servisi çalışmıyor',
        service_missing_hint: 'Kurulum: kota dizininde `make install`',
        service_error: 'Servise ulaşılamadı',
        connect: 'Bağlan',
        reconnect: 'Yeniden bağlan',
        add_key: 'Anahtar ekle',
        connect_failed: 'Giriş terminali açılamadı',
        connect_opened: 'Giriş için terminal açıldı',
        connect_opened_body: 'Giriş tamamlanınca Kota bağlantıyı kendiliğinden algılar.',
        left: 'kaldı',
        weekly_share: 'Haftalık pay',
        stale: 'Son veri',
        local_log: 'yerel log',
        reset_done: 'yenilendi',
        // kutucuklar
        tile_running: 'Çalışan',
        tile_live: 'Canlı',
        tile_tokens: 'Bugün',
        tile_tokens_unit: 'token',
        tile_reset: 'Yenilenme',
        tile_none: '—',
        // kartlar
        card_usage: 'AI kullanımı',
        card_activity: 'Son 24 saat',
        sessions: 'oturum',
        now: 'Şimdi',
        no_activity: 'Son 24 saatte yerel etkinlik yok',
        card_live: 'Canlı',
        agents_unit: 'ajan',
        also: 'ayrıca',
        tokens_unit: 'token',
        range_24h: '24 sa',
        range_7d: '7 gün',
        range_30d: '30 gün',
        card_history: 'Kullanım geçmişi',
        vs_last_week_up: 'geçen haftadan %{n} fazla',
        vs_last_week_down: 'geçen haftadan %{n} az',
        vs_last_week_same: 'geçen haftayla aynı',
        this_week: 'Bu hafta',
        daily_avg: 'günlük ort.',
        notify_forecast: 'yakında biter',
        notify_forecast_body: 'Bu hızla {when} biter; yenilenme {reset}.',
        prefs_notify_forecast: 'Tükenme uyarısı',
        prefs_notify_forecast_sub: 'Bu hızla limit, yenilenmeden önce bitecekse önceden haber ver',
        prefs_forecast_minutes: 'Kaç dakika önce',
        prefs_panel_icon: 'Panel simgesi',
        icon_provider: 'AI platformu logosu',
        icon_kota: 'Kota logosu',
        icon_both: 'Kota + AI logoları',
        prefs_bar_colors: 'Çubuk renkleri',
        prefs_bar_colors_sub: 'Limite duyarlı: doldukça yeşil → sarı → turuncu → kırmızı',
        bars_usage: 'Limite duyarlı',
        bars_brand: 'Marka rengi',
        prefs_theme: 'Tema',
        theme_auto: 'Sistemle aynı',
        theme_dark: 'Koyu',
        theme_light: 'Açık',
        prefs_accounts: 'Hesaplar',
        prefs_accounts_sub: 'Birden fazla hesap arasında geçiş. Kimlik bilgileri GNOME Anahtarlığı\'nda saklanır.',
        prefs_account_save: 'Etkin hesabı kaydet',
        prefs_account_save_sub: 'Profil adı (boş: e-posta)',
        prefs_account_use: 'Kullan',
        prefs_account_active: 'Etkin',
        prefs_account_current: 'Şu an girişli',
        prefs_account_none: 'Kayıtlı profil yok',
        prefs_search: 'Ayarlarda ara',
        prefs_search_empty: 'Sonuç yok',
        just_now: 'az önce',
        no_agents: 'Çalışan AI ajanı yok',
        working: 'Çalışıyor',
        idle: 'Boşta',
        ports_unit: 'port',
        stop: 'Durdur',
        open_folder: 'Klasörü aç',
        filter_hint: 'Ada, projeye veya porta göre filtrele…',
        no_match: 'Eşleşen yok',
        balance: 'Bakiye',
        spend: 'Bu ay',
        budget: 'bütçe',
        limits: 'Limitler',
        // bildirimler
        notify_reset: 'yenilendi',
        notify_expired: 'oturum süresi doldu',
        // tercihler
        prefs_providers_page: 'Sağlayıcılar',
        prefs_providers_desc: 'Takip edilecek AI hizmetlerini açın. API anahtarları GNOME Anahtarlığı’nda saklanır, hiçbir dosyaya yazılmaz.',
        prefs_service_down: 'Kota servisine ulaşılamadı. Sağlayıcı ayarları için servis çalışmalı: systemctl --user start kota',
        prefs_enabled: 'Takip et',
        prefs_api_key: 'API anahtarı',
        prefs_admin_key: 'Yönetici (admin) anahtarı',
        prefs_github_token: 'GitHub token',
        prefs_key_saved: 'Anahtar kayıtlı',
        prefs_key_missing: 'Anahtar yok',
        prefs_key_clear: 'Anahtarı sil',
        prefs_key_get: 'Anahtar al',
        prefs_login: 'Giriş',
        prefs_login_sub: 'Sağlayıcının kendi CLI girişini terminalde açar',
        prefs_budget: 'Aylık bütçe ($)',
        prefs_budget_sub: 'Harcama bu tutara oranla yüzde olarak gösterilir (0 = kapalı)',
        prefs_in_panel: 'Panelde göster',
        prefs_panel_window: 'Paneldeki değer',
        auto_highest: 'Otomatik (en yüksek)',
        follow_global: 'Genel ayar (Görünüm → Üst panel)',
        prefs_panel_windows: 'Pencereler',
        prefs_panel_windows_sub: 'Birden fazla limit varsa panelde hangileri görünsün (sağlayıcı ayarı bunu geçersiz kılar)',
        pw_all: 'Hepsi, baş harfle (G %14 · H %22)',
        pw_auto: 'Yalnızca en yüksek',
        prefs_letters: 'Pencere harfleri',
        prefs_letters_sub: 'Tek değer gösterilirken de baş harfi ekle (G = gün/oturum, H = hafta, A = ay)',
        prefs_shows_plan_limits: 'Plan limitleri (yüzde, yenilenme)',
        prefs_shows_balance: 'Bakiye',
        prefs_shows_spend: 'Aylık harcama + bütçe yüzdesi',
        prefs_appearance_page: 'Görünüm',
        prefs_panel: 'Üst panel',
        prefs_position: 'Konum',
        pos_left: 'Sol',
        pos_center: 'Orta',
        pos_right: 'Sağ',
        prefs_style: 'Biçim',
        style_icon: 'Logo + yüzde',
        style_ring: 'Halka + yüzde',
        style_text: 'Yalnızca yüzde',
        prefs_mode: 'Gösterim',
        mode_each: 'Seçili sağlayıcıların hepsi',
        mode_max: 'Yalnızca en yüksek değer',
        prefs_remaining: 'Kalan yüzdeyi göster',
        prefs_remaining_sub: 'Kullanılan yerine kalan kotayı göster',
        prefs_colored: 'Renkli logolar',
        prefs_colored_sub: 'Panelde marka renkleri (kapalıyken tema rengi)',
        prefs_menu: 'Açılır pencere',
        prefs_width: 'Genişlik (px)',
        prefs_columns: 'Kullanım sütunları',
        prefs_accent: 'Vurgu rengi',
        prefs_sections: 'Bölümler',
        sec_tiles: 'Özet kutucukları',
        sec_usage: 'AI kullanımı halkaları',
        sec_activity: 'Son 24 saat grafiği',
        sec_agents: 'Çalışan ajanlar',
        sec_filter: 'Filtre kutusu',
        prefs_notify_page: 'Bildirimler',
        prefs_notify: 'Eşik bildirimleri',
        prefs_notify_sub: 'Bir pencere uyarı ve kritik eşiği geçince bir kez bildir',
        prefs_notify_reset: 'Sıfırlanma bildirimi',
        prefs_notify_reset_sub: 'Eşiği geçmiş bir limit sıfırlanınca haber ver',
        prefs_notify_expired: 'Oturum bildirimi',
        prefs_notify_expired_sub: 'Bir sağlayıcının oturumu düşünce haber ver',
        prefs_thresholds: 'Eşikler (servis)',
        prefs_warning: 'Uyarı (%)',
        prefs_critical: 'Kritik (%)',
        prefs_service_page: 'Servis',
        prefs_polling: 'Sorgulama',
        prefs_interval: 'Yenileme aralığı (sn)',
        prefs_paused: 'Duraklat',
        prefs_paused_sub: 'Düzenli sorguyu durdur (elle yenileme çalışır)',
        prefs_claude_refresh: 'Claude token yenileme',
        prefs_claude_refresh_sub: 'Süresi dolan token’ı Claude Code ile aynı kilitle yenile',
        prefs_local_logs: 'Yerel log taraması',
        prefs_local_logs_sub: 'Token sayıları ve 24 saat grafiği için ~/.claude ve ~/.codex logları',
        prefs_general: 'Genel',
        prefs_language: 'Dil',
        lang_auto: 'Sistem dili',
        prefs_shortcut: 'Klavye kısayolu',
        prefs_shortcut_sub: 'Paneli aç/kapat (ör. <Super><Alt>k; boş = kapalı)',
        prefs_open_config: 'config.toml’u aç',
        prefs_restart: 'Servisi yeniden başlat',
        prefs_about: 'Hakkında',
        open_kota: 'Kota’yı aç',
        limit_reached: 'Limit doldu',
        pace_fast: 'Hızlı gidiyorsun',
        pace_slow: 'Rahatsın',
        prefs_placement_page: 'Yerleşim',
        prefs_place_panel: 'Üst panel göstergesi',
        prefs_place_panel_sub: 'Sistem tepesinde logo + yüzde',
        prefs_panel_index: 'Sıra',
        prefs_panel_index_sub: 'Seçili alandaki konumu (0 = en baş, -1 = en son)',
        prefs_place_qs: 'Hızlı Ayarlar',
        prefs_place_qs_desc: 'Wi-Fi, Bluetooth gibi kutucukların arasında',
        prefs_qs_enabled: 'Hızlı Ayarlar’da göster',
        prefs_qs_mode: 'Kutucuklar',
        qs_single: 'Tek kutucuk (Kota)',
        qs_providers: 'Her sağlayıcı ayrı kutucuk',
        prefs_qs_mode_sub: 'Tek kutucukta tıklamak takibi duraklatır, ok düğmesi ayrıntıları açar',
        prefs_qs_indicator: 'Sistem simgelerinde göster',
        prefs_qs_indicator_sub: 'Pil ve Wi-Fi simgelerinin yanında logo + yüzde',
        prefs_qs_full: 'Menüde tüm pencereler',
        prefs_qs_full_sub: 'Kapalıyken yalnızca paneldeki değer',
        prefs_place_datemenu: 'Tarih ve bildirim penceresi',
        prefs_place_datemenu_desc: 'Saate tıklayınca açılan pencerede (takvim, hava durumu, bildirimler)',
        prefs_datemenu_enabled: 'Tarih penceresinde göster',
        prefs_datemenu_position: 'Konum',
        dm_right_top: 'Sağ sütun · üstte (takvimin altı)',
        dm_right_bottom: 'Sağ sütun · altta (hava durumunun altı)',
        dm_left_top: 'Sol sütun · bildirimlerin üstü',
        dm_left_bottom: 'Sol sütun · bildirimlerin altı',
        prefs_datemenu_detail: 'Ayrıntı',
        dm_compact: 'Sade (sağlayıcı başına tek çizgi)',
        dm_full: 'Tam (tüm pencereler)',
        prefs_datemenu_selected: 'Yalnızca paneldeki sağlayıcılar',
        prefs_background: 'Arka plan',
        bg_auto: 'Otomatik (Blur My Shell ile uyumlu)',
        bg_translucent: 'Yarı saydam',
        bg_opaque: 'Opak',
        prefs_background_sub: 'Otomatik: Blur My Shell açılır pencere bulanıklığı açıksa saydamlaşır ve onun köşe yarıçapını kullanır',
        prefs_opacity: 'Saydamlık katmanı (%)',
        prefs_usage_style: 'Kullanım kartı',
        style_rings: 'Halkalar',
        style_bars: 'İlerleme çizgileri',
        prefs_pace: 'Tempo işareti',
        prefs_pace_sub: 'Çizgide, eşit hızla kullansaydın şu an nerede olacağını gösterir',
    },
    en: {
        app_tagline: 'AI usage',
        refresh_now: 'Refresh now',
        settings: 'Settings',
        pause: 'Pause',
        resume: 'Resume',
        paused: 'Paused',
        updated_at: 'Updated',
        loading: 'Loading…',
        no_windows: 'No limit data',
        no_providers: 'No providers enabled — Settings → Providers',
        service_missing: 'The Kota service is not running',
        service_missing_hint: 'Install it: run `make install` in the kota directory',
        service_error: 'Could not reach the service',
        connect: 'Connect',
        reconnect: 'Reconnect',
        add_key: 'Add key',
        connect_failed: 'Could not open the sign-in terminal',
        connect_opened: 'Opened a terminal to sign in',
        connect_opened_body: 'Kota detects the new session automatically.',
        left: 'left',
        weekly_share: 'Weekly share',
        stale: 'Last data',
        local_log: 'local log',
        reset_done: 'renewed',
        tile_running: 'Running',
        tile_live: 'Live',
        tile_tokens: 'Today',
        tile_tokens_unit: 'tokens',
        tile_reset: 'Next reset',
        tile_none: '—',
        card_usage: 'AI usage',
        card_activity: 'Last 24 hours',
        sessions: 'sessions',
        now: 'Now',
        no_activity: 'No local activity in the last 24 hours',
        card_live: 'Live',
        agents_unit: 'agents',
        also: 'also',
        tokens_unit: 'tokens',
        range_24h: '24h',
        range_7d: '7 days',
        range_30d: '30 days',
        card_history: 'Usage history',
        vs_last_week_up: '{n}% more than last week',
        vs_last_week_down: '{n}% less than last week',
        vs_last_week_same: 'same as last week',
        this_week: 'This week',
        daily_avg: 'daily avg.',
        notify_forecast: 'running out soon',
        notify_forecast_body: 'At this pace it runs out {when}; resets {reset}.',
        prefs_notify_forecast: 'Run-out warning',
        prefs_notify_forecast_sub: 'Warn ahead when the limit will run out before it resets',
        prefs_forecast_minutes: 'Minutes ahead',
        prefs_panel_icon: 'Panel icon',
        icon_provider: 'AI platform logo',
        icon_kota: 'Kota logo',
        icon_both: 'Kota + AI logos',
        prefs_bar_colors: 'Bar colors',
        prefs_bar_colors_sub: 'Limit-aware: green → yellow → orange → red as it fills',
        bars_usage: 'Limit-aware',
        bars_brand: 'Brand color',
        prefs_theme: 'Theme',
        theme_auto: 'Follow system',
        theme_dark: 'Dark',
        theme_light: 'Light',
        prefs_accounts: 'Accounts',
        prefs_accounts_sub: 'Switch between several accounts. Credentials are kept in GNOME Keyring.',
        prefs_account_save: 'Save current account',
        prefs_account_save_sub: 'Profile name (empty: email)',
        prefs_account_use: 'Use',
        prefs_account_active: 'Active',
        prefs_account_current: 'Logged in now',
        prefs_account_none: 'No saved profiles',
        prefs_search: 'Search settings',
        prefs_search_empty: 'No results',
        just_now: 'just now',
        no_agents: 'No AI agents running',
        working: 'Working',
        idle: 'Idle',
        ports_unit: 'ports',
        stop: 'Stop',
        open_folder: 'Open folder',
        filter_hint: 'Filter by name, project or port…',
        no_match: 'No matches',
        balance: 'Balance',
        spend: 'This month',
        budget: 'budget',
        limits: 'Limits',
        notify_reset: 'reset',
        notify_expired: 'session expired',
        prefs_providers_page: 'Providers',
        prefs_providers_desc: 'Turn on the AI services to track. API keys are stored in the GNOME Keyring, never in a file.',
        prefs_service_down: 'Could not reach the Kota service. Provider settings need it running: systemctl --user start kota',
        prefs_enabled: 'Track',
        prefs_api_key: 'API key',
        prefs_admin_key: 'Admin key',
        prefs_github_token: 'GitHub token',
        prefs_key_saved: 'Key stored',
        prefs_key_missing: 'No key',
        prefs_key_clear: 'Remove key',
        prefs_key_get: 'Get a key',
        prefs_login: 'Sign in',
        prefs_login_sub: 'Opens the provider’s own CLI sign-in in a terminal',
        prefs_budget: 'Monthly budget ($)',
        prefs_budget_sub: 'Spend is shown as a percentage of this (0 = off)',
        prefs_in_panel: 'Show in panel',
        prefs_panel_window: 'Panel value',
        auto_highest: 'Automatic (highest)',
        follow_global: 'Global setting (Appearance → Top bar)',
        prefs_panel_windows: 'Windows',
        prefs_panel_windows_sub: 'Which limits to show when a provider has several (a per-provider choice overrides this)',
        pw_all: 'All, with initials (D 14% · W 22%)',
        pw_auto: 'Only the highest',
        prefs_letters: 'Window letters',
        prefs_letters_sub: 'Add the initial even when one value is shown (D = day/session, W = week, M = month)',
        prefs_shows_plan_limits: 'Plan limits (percent, resets)',
        prefs_shows_balance: 'Balance',
        prefs_shows_spend: 'Monthly spend + budget percent',
        prefs_appearance_page: 'Appearance',
        prefs_panel: 'Top bar',
        prefs_position: 'Position',
        pos_left: 'Left',
        pos_center: 'Center',
        pos_right: 'Right',
        prefs_style: 'Style',
        style_icon: 'Logo + percent',
        style_ring: 'Ring + percent',
        style_text: 'Percent only',
        prefs_mode: 'Display',
        mode_each: 'All selected providers',
        mode_max: 'Highest value only',
        prefs_remaining: 'Show remaining percentage',
        prefs_remaining_sub: 'Show what is left instead of what is used',
        prefs_colored: 'Colored logos',
        prefs_colored_sub: 'Brand colors in the top bar (off: theme color)',
        prefs_menu: 'Popup',
        prefs_width: 'Width (px)',
        prefs_columns: 'Usage columns',
        prefs_accent: 'Accent color',
        prefs_sections: 'Sections',
        sec_tiles: 'Summary tiles',
        sec_usage: 'AI usage rings',
        sec_activity: 'Last 24 hours chart',
        sec_agents: 'Running agents',
        sec_filter: 'Filter box',
        prefs_notify_page: 'Notifications',
        prefs_notify: 'Threshold notifications',
        prefs_notify_sub: 'Notify once when a window crosses the warning and critical thresholds',
        prefs_notify_reset: 'Reset notification',
        prefs_notify_reset_sub: 'Tell me when a limit that crossed a threshold resets',
        prefs_notify_expired: 'Session notification',
        prefs_notify_expired_sub: 'Tell me when a provider’s session drops',
        prefs_thresholds: 'Thresholds (service)',
        prefs_warning: 'Warning (%)',
        prefs_critical: 'Critical (%)',
        prefs_service_page: 'Service',
        prefs_polling: 'Polling',
        prefs_interval: 'Refresh interval (s)',
        prefs_paused: 'Pause',
        prefs_paused_sub: 'Stop regular polling (manual refresh still works)',
        prefs_claude_refresh: 'Claude token refresh',
        prefs_claude_refresh_sub: 'Renew expired tokens using Claude Code’s own lock',
        prefs_local_logs: 'Local log scanning',
        prefs_local_logs_sub: 'Token counts and the 24-hour chart from ~/.claude and ~/.codex logs',
        prefs_general: 'General',
        prefs_language: 'Language',
        lang_auto: 'System language',
        prefs_shortcut: 'Keyboard shortcut',
        prefs_shortcut_sub: 'Toggle the popup (e.g. <Super><Alt>k; empty = off)',
        prefs_open_config: 'Open config.toml',
        prefs_restart: 'Restart service',
        prefs_about: 'About',
        open_kota: 'Open Kota',
        limit_reached: 'Limit reached',
        pace_fast: 'Going fast',
        pace_slow: 'Plenty left',
        prefs_placement_page: 'Placement',
        prefs_place_panel: 'Top bar indicator',
        prefs_place_panel_sub: 'Logo + percent in the top bar',
        prefs_panel_index: 'Order',
        prefs_panel_index_sub: 'Position inside the chosen area (0 = first, -1 = last)',
        prefs_place_qs: 'Quick Settings',
        prefs_place_qs_desc: 'Among tiles like Wi-Fi and Bluetooth',
        prefs_qs_enabled: 'Show in Quick Settings',
        prefs_qs_mode: 'Tiles',
        qs_single: 'Single tile (Kota)',
        qs_providers: 'One tile per provider',
        prefs_qs_mode_sub: 'On the single tile, clicking pauses tracking and the arrow opens details',
        prefs_qs_indicator: 'Show among system icons',
        prefs_qs_indicator_sub: 'Logo + percent next to the battery and Wi-Fi icons',
        prefs_qs_full: 'All windows in the menu',
        prefs_qs_full_sub: 'Off: only the panel value',
        prefs_place_datemenu: 'Date and notifications popup',
        prefs_place_datemenu_desc: 'In the popup opened from the clock (calendar, weather, notifications)',
        prefs_datemenu_enabled: 'Show in the date popup',
        prefs_datemenu_position: 'Position',
        dm_right_top: 'Right column · top (below calendar)',
        dm_right_bottom: 'Right column · bottom (below weather)',
        dm_left_top: 'Left column · above notifications',
        dm_left_bottom: 'Left column · below notifications',
        prefs_datemenu_detail: 'Detail',
        dm_compact: 'Compact (one line per provider)',
        dm_full: 'Full (every window)',
        prefs_datemenu_selected: 'Only providers shown in the panel',
        prefs_background: 'Background',
        bg_auto: 'Automatic (Blur My Shell aware)',
        bg_translucent: 'Translucent',
        bg_opaque: 'Opaque',
        prefs_background_sub: 'Automatic: turns translucent and uses its corner radius when Blur My Shell popup blur is on',
        prefs_opacity: 'Tint layer (%)',
        prefs_usage_style: 'Usage card',
        style_rings: 'Rings',
        style_bars: 'Progress lines',
        prefs_pace: 'Pace marker',
        prefs_pace_sub: 'Shows where you would be on the line at an even pace',
    },
};

const TR_MONTHS = ['Oca', 'Şub', 'Mar', 'Nis', 'May', 'Haz', 'Tem', 'Ağu', 'Eyl', 'Eki', 'Kas', 'Ara'];
const EN_MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const TR_DAYS = ['Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt', 'Paz'];
const EN_DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

const TR_WINDOWS = {
    session: '5 saatlik oturum',
    daily: 'Günlük',
    weekly: 'Haftalık',
    weekly_opus: 'Haftalık · Opus',
    weekly_sonnet: 'Haftalık · Sonnet',
    weekly_oauth_apps: 'Haftalık · OAuth uygulamaları',
    weekly_cowork: 'Haftalık · Cowork',
    monthly: '30 günlük',
    budget: 'Aylık bütçe',
    premium_interactions: 'Premium istekler',
    chat: 'Sohbet',
    completions: 'Tamamlamalar',
    tools_monthly: 'Araç çağrıları (aylık)',
    key_limit: 'Anahtar limiti',
    key_daily: 'Anahtar limiti (günlük)',
    key_weekly: 'Anahtar limiti (haftalık)',
    key_monthly: 'Anahtar limiti (aylık)',
};

const SHORT_TR = {session: '5 sa', weekly: 'Hafta', monthly: '30 gün', daily: 'Gün', budget: 'Bütçe'};
const SHORT_EN = {session: '5h', weekly: 'Week', monthly: '30d', daily: 'Day', budget: 'Budget'};

/** 'auto' → sistem dili; 'tr' ile başlıyorsa Türkçe. */
export function resolveLanguage(setting) {
    if (setting === 'tr' || setting === 'en')
        return setting;
    for (const n of GLib.get_language_names()) {
        const v = n.toLowerCase();
        if (v === 'c' || v.startsWith('c.') || v === 'posix')
            continue;
        return v.startsWith('tr') ? 'tr' : 'en';
    }
    return 'en';
}

/** ISO 8601 → milisaniye (nanosaniye hassasiyeti ms'ye kırpılır). */
export function parseTime(iso) {
    if (!iso)
        return NaN;
    return Date.parse(String(iso).replace(/(\.\d{3})\d+/, '$1'));
}

function localDateTime(ms) {
    return GLib.DateTime.new_from_unix_local(Math.floor(ms / 1000));
}

function dayIndex(dt) {
    const midnight = GLib.DateTime.new_local(dt.get_year(), dt.get_month(), dt.get_day_of_month(), 0, 0, 0);
    return Math.round(midnight.to_unix() / 86400);
}

export class Strings {
    constructor(lang) {
        this.lang = lang === 'tr' ? 'tr' : 'en';
        this._t = TEXT[this.lang];
    }

    get tr() {
        return this.lang === 'tr';
    }

    t(key) {
        return this._t[key] ?? TEXT.en[key] ?? key;
    }

    /** Türkçede yüzde işareti sayıdan önce gelir. */
    percent(value, estimated = false) {
        const n = Math.round(value);
        const tilde = estimated ? '~' : '';
        return this.tr ? `${tilde}%${n}` : `${tilde}${n}%`;
    }

    windowLabel(w) {
        if (!this.tr)
            return w.label;
        if (w.id.startsWith('code_review:'))
            return `Kod inceleme · ${this._baseLabel(w.id.slice('code_review:'.length), w.label)}`;
        if (w.id.startsWith('extra:')) {
            const name = w.label.split(' · ')[0];
            return `${name} · ${this._baseLabel(w.id.split(':').pop(), w.label)}`;
        }
        if (w.id.startsWith('model:'))
            return w.label;
        return this._baseLabel(w.id, w.label);
    }

    /** Halka altındaki kısa etiket ("5 sa", "Hafta", model adı…). */
    windowShort(w) {
        const map = this.tr ? SHORT_TR : SHORT_EN;
        if (map[w.id])
            return map[w.id];
        if (w.id.startsWith('model:'))
            return w.label.replace(/^gemini-/, '');
        return this.windowLabel(w);
    }

    _baseLabel(id, fallback) {
        if (TR_WINDOWS[id])
            return TR_WINDOWS[id];
        let m = id.match(/^days_(\d+)$/);
        if (m)
            return `${m[1]} günlük`;
        m = id.match(/^hours_(\d+)$/);
        if (m)
            return `${m[1]} saatlik`;
        return fallback;
    }

    status(p) {
        const code = p.code ?? '';
        const tr = this.tr;
        switch (p.status) {
        case 'ok': return tr ? 'bağlı' : 'connected';
        case 'not_connected':
            if (code === 'no_api_key')
                return tr ? 'anahtar gerekli' : 'key needed';
            return tr ? 'bağlı değil' : 'not connected';
        case 'expired':
            if (code === 'key_rejected')
                return tr ? 'anahtar reddedildi' : 'key rejected';
            if (code === 'relogin_required' || code === 'token_rejected')
                return tr ? 'yeniden giriş gerekli' : 'sign-in required';
            return tr ? 'oturum süresi doldu' : 'session expired';
        case 'unsupported':
            if (code === 'api_key_mode')
                return tr ? 'API anahtarı modu' : 'API key mode';
            if (code === 'ineligible')
                return tr ? 'hesap uygun değil' : 'not eligible';
            return tr ? 'desteklenmiyor' : 'unsupported';
        default: {
            const map = {
                rate_limited: ['hız sınırı, bekleniyor', 'rate limited'],
                network: ['ağ hatası', 'network error'],
                timeout: ['zaman aşımı', 'timed out'],
                parse_error: ['beklenmeyen yanıt', 'unexpected response'],
                refresh_busy: ['token yenileniyor', 'refreshing token'],
                refresh_failed: ['token yenilenemedi', 'token refresh failed'],
                credentials_unreadable: ['giriş dosyası okunamadı', 'credentials unreadable'],
            };
            const pair = map[code] ?? ['hata', 'error'];
            return tr ? pair[0] : pair[1];
        }
        }
    }

    connectLabel(p) {
        if (p.code === 'no_api_key' || p.code === 'key_rejected')
            return this.t('add_key');
        return p.status === 'not_connected' ? this.t('connect') : this.t('reconnect');
    }

    /** Yerel saatle kısa zaman: bugün `23:20`, yarın `yarın 07:59`, hafta içi `Per 07:59`, sonra `13 Kas 07:59`. */
    when(ms, nowMs = Date.now()) {
        // API sıfırlanma anını milisaniye oynamasıyla döndürüyor; en yakın dakikaya yuvarla.
        const dt = localDateTime(ms + 30000);
        const now = localDateTime(nowMs);
        const clock = dt.format('%H:%M');
        const days = dayIndex(dt) - dayIndex(now);
        if (days === 0)
            return clock;
        if (days === 1)
            return this.tr ? `yarın ${clock}` : `tomorrow ${clock}`;
        if (days === -1)
            return this.tr ? `dün ${clock}` : `yesterday ${clock}`;
        if (days > 1 && days < 7) {
            const wd = dt.get_day_of_week() - 1;
            return `${(this.tr ? TR_DAYS : EN_DAYS)[wd]} ${clock}`;
        }
        const m = dt.get_month() - 1;
        return this.tr
            ? `${dt.get_day_of_month()} ${TR_MONTHS[m]} ${clock}`
            : `${EN_MONTHS[m]} ${dt.get_day_of_month()} ${clock}`;
    }

    /** Saat etiketi (grafik ekseni): `dün 01:00` / `01:00`. */
    hourLabel(ms, nowMs = Date.now()) {
        return this.when(ms - 30000, nowMs);
    }

    /** `2 sa 50 dk` / `2h 50m` (en fazla iki birim). */
    duration(seconds) {
        const mins = Math.max(0, Math.floor(seconds / 60));
        const d = Math.floor(mins / 1440), h = Math.floor((mins % 1440) / 60), m = mins % 60;
        const [ud, uh, um] = this.tr ? [' g', ' sa', ' dk'] : ['d', 'h', 'm'];
        if (d > 0)
            return h > 0 ? `${d}${ud} ${h}${uh}` : `${d}${ud}`;
        if (h > 0)
            return m > 0 ? `${h}${uh} ${m}${um}` : `${h}${uh}`;
        if (m > 0)
            return `${m}${um}`;
        return this.tr ? '<1 dk' : '<1m';
    }

    /** Kutucuk için tek birimli kısa süre: `2 g`, `4 sa`, `37 dk`. */
    shortDuration(seconds) {
        const mins = Math.max(0, Math.floor(seconds / 60));
        const [ud, uh, um] = this.tr ? [' g', ' sa', ' dk'] : ['d', 'h', 'm'];
        if (mins >= 1440)
            return `${Math.floor(mins / 1440)}${ud}`;
        if (mins >= 60)
            return `${Math.floor(mins / 60)}${uh}`;
        return `${Math.max(1, mins)}${um}`;
    }

    /** Halka altı için kısa göreli sıfırlanma: "4 sa 10 dk sonra" / "in 4h 10m". */
    resetsIn(iso, nowMs = Date.now()) {
        const ms = parseTime(iso);
        if (Number.isNaN(ms))
            return '';
        const left = (ms - nowMs) / 1000;
        if (left <= 0)
            return this.t('reset_done');
        const dur = this.duration(left + 30);
        return this.tr ? `${dur} sonra yenilenir` : `resets in ${dur}`;
    }

    resets(iso, nowMs = Date.now()) {
        const ms = parseTime(iso);
        if (Number.isNaN(ms))
            return '';
        const left = (ms - nowMs) / 1000;
        if (left <= 0)
            return this.t('reset_done');
        const when = this.when(ms, nowMs);
        const dur = this.duration(left + 30);
        return this.tr ? `Yenilenme: ${when} (${dur})` : `Resets ${when} (${dur})`;
    }

    /** "bu hızla 21:40'ta dolar" */
    exhausts(iso, nowMs = Date.now()) {
        const ms = parseTime(iso);
        if (Number.isNaN(ms))
            return '';
        const when = this.when(ms, nowMs);
        return this.tr ? `Bu hızla biter: ${when}` : `At this pace, runs out ${when}`;
    }

    tokens(n) {
        if (n < 1000)
            return String(n);
        let v, unit;
        if (n < 1e6)
            [v, unit] = [n / 1e3, 'K'];
        else if (n < 1e9)
            [v, unit] = [n / 1e6, 'M'];
        else
            [v, unit] = [n / 1e9, 'B'];
        let s = v >= 100 ? v.toFixed(0) : v.toFixed(1);
        if (this.tr)
            s = s.replace('.', ',');
        return `${s}${unit}`;
    }

    bytes(n) {
        if (n >= 1024 ** 3) {
            const s = (n / 1024 ** 3).toFixed(1);
            return `${this.tr ? s.replace('.', ',') : s} GB`;
        }
        return `${Math.round(n / 1024 ** 2)} MB`;
    }

    money(amount, currency) {
        const sym = {USD: '$', CNY: '¥', EUR: '€', TRY: '₺'}[String(currency).toUpperCase()] ?? '';
        let n = amount >= 1000 ? amount.toFixed(0) : amount.toFixed(2);
        if (this.tr)
            n = n.replace('.', ',');
        return sym ? `${sym}${n}` : `${n} ${currency}`;
    }

    localLine(local) {
        if (!local)
            return null;
        const today = local.today?.total ?? 0;
        const win = local.window?.total ?? 0;
        if (!today && !win)
            return null;
        const t = this.tokens(today), w = win ? this.tokens(win) : null;
        if (this.tr)
            return w ? `Yerel log: bugün ${t} token · bu oturumda ${w}` : `Yerel log: bugün ${t} token`;
        return w ? `Local logs: ${t} tokens today · ${w} this session` : `Local logs: ${t} tokens today`;
    }

    staleSince(iso, nowMs = Date.now()) {
        const ms = parseTime(iso);
        const when = Number.isNaN(ms) ? '?' : this.when(ms, nowMs);
        return `${this.t('stale')}: ${when}`;
    }

    /** Halka altındaki "as of" metni: eski veride "Per itibarıyla". */
    asOf(iso, nowMs = Date.now()) {
        const ms = parseTime(iso);
        if (Number.isNaN(ms))
            return '';
        const when = this.when(ms, nowMs);
        return this.tr ? `Son veri: ${when}` : `Last data: ${when}`;
    }

    /** `{n}` gibi yer tutucuları doldurur. */
    fmt(key, vars) {
        return this.t(key).replace(/\{(\w+)\}/g, (_m, k) => String(vars[k] ?? ''));
    }

    /** "Pzt", "12 Kas" gibi kısa gün etiketi (yerel). */
    dayLabel(ms, long = false) {
        const dt = GLib.DateTime.new_from_unix_local(Math.floor(ms / 1000));
        if (!long)
            return (this.tr ? ['Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt', 'Paz'] : ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'])[dt.get_day_of_week() - 1];
        const months = this.tr ? ['Oca', 'Şub', 'Mar', 'Nis', 'May', 'Haz', 'Tem', 'Ağu', 'Eyl', 'Eki', 'Kas', 'Ara']
            : ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
        return this.tr ? `${dt.get_day_of_month()} ${months[dt.get_month() - 1]}` : `${months[dt.get_month() - 1]} ${dt.get_day_of_month()}`;
    }

    /** "az önce", "5 dk önce", "2 sa önce" */
    ago(iso, nowMs = Date.now()) {
        const ms = parseTime(iso);
        if (Number.isNaN(ms))
            return '';
        const secs = (nowMs - ms) / 1000;
        if (secs < 90)
            return this.t('just_now');
        return this.tr ? `${this.shortDuration(secs)} önce` : `${this.shortDuration(secs)} ago`;
    }

    /**
     * Pencerenin dile göre baş harfi: gün (≤1 gün, 5 saatlik oturum dahil),
     * hafta, ay. TR: G/H/A · EN: D/W/M. Model kotalarında model adının ilk harfi.
     */
    windowLetter(w) {
        const L = this.tr ? {day: 'G', week: 'H', month: 'A'} : {day: 'D', week: 'W', month: 'M'};
        const secs = w.window_seconds;
        if (secs) {
            if (secs <= 86400 * 1.5)
                return L.day;
            if (secs <= 86400 * 10)
                return L.week;
            return L.month;
        }
        if (w.id === 'session' || w.id === 'daily' || /^hours_/.test(w.id))
            return L.day;
        if (w.id === 'weekly')
            return L.week;
        if (w.id === 'monthly' || /^days_/.test(w.id))
            return L.month;
        return (this.windowShort(w)[0] ?? '?').toUpperCase();
    }

    idleFor(iso, nowMs = Date.now()) {
        const ms = parseTime(iso);
        if (Number.isNaN(ms) || nowMs - ms < 90000)
            return this.t('working');
        return `${this.t('idle')} ${this.duration((nowMs - ms) / 1000)}`;
    }

    detailLabel(d) {
        if (this.tr) {
            const m = {extra_usage: 'Ek kullanım', credits: 'Kredi', key_usage: 'Anahtar kullanımı', voucher: 'Kupon'};
            if (m[d.id])
                return m[d.id];
        }
        return d.label;
    }

    /** Ayrıntı değerindeki "53%" biçimini dile uydurur. */
    detailValue(v) {
        const m = String(v).match(/^(\d+(?:\.\d+)?)%$/);
        return m ? this.percent(Number(m[1])) : String(v);
    }

    notifyTitle(provider, w) {
        return `${provider.name}: ${this.windowLabel(w)} ${this.percent(w.used_percent)}`;
    }

    shows(kind) {
        return this.t(`prefs_shows_${kind}`);
    }
}
