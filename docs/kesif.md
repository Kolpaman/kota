# Keşif notları (Aşama 1) — 6 Kasım 2026

Bu makinede (Fedora 44, GNOME 50.6, Claude Code 2.1.319) gerçek yanıtlar
kaydedildi. Kimlik bilgisi içermeyen/maskelenmiş kopyalar:
`crates/kota-core/tests/fixtures/`.

## Claude — `GET https://api.anthropic.com/api/oauth/usage`

- Başlıklar: `Authorization: Bearer <claudeAiOauth.accessToken>`,
  `anthropic-beta: oauth-2025-04-20`. 200 döndü.
- Yeni şema: `limits: [{kind: "session"|"weekly_all"|…, group, percent,
  severity, resets_at, scope, is_active}]`.
- Eski alanlar hâlâ var: `five_hour`, `seven_day`, `seven_day_opus`,
  `seven_day_sonnet`, … → `{utilization, resets_at}` (+ yeni `*_dollars`).
- Ek: `extra_usage`, `spend`, `seven_day_breakdown.rows` (Claude Code / Chats /
  Cowork payları), çok sayıda kod adlı `null` alan (şema sık değişiyor).
- `resets_at` milisaniye oynuyor (`…:59.81`), arayüz en yakın dakikaya yuvarlar.

## Claude — token yenileme

- `~/.claude/.credentials.json`: `claudeAiOauth.{accessToken, refreshToken,
  expiresAt(ms), refreshTokenExpiresAt, scopes[], subscriptionType,
  subscriptionTypeSource, rateLimitTier}`. Erişim token'ı **8 saat** geçerli.
- Claude Code binary'sinden doğrulanan sabitler: `TOKEN_URL =
  https://platform.claude.com/v1/oauth/token`, `CLIENT_ID =
  9d1c250a-e61b-44d9-88ed-5944d1962f5e`, gövde JSON:
  `{grant_type: "refresh_token", refresh_token, client_id, scope}`.
- Kilit: proper-lockfile, `~/.claude/.oauth_refresh.lock` (stale 60 sn,
  update 5 sn) + eski kilit `realpath(~/.claude).lock`. Kilit altında dosyayı
  yeniden okur; erişim token'ı değişmişse "race resolved" sayıp istek atmaz.
  Kaydederken compare-and-swap: dosyadaki refresh token gönderilenden farklıysa
  "adopted sibling". Kota aynı protokolü uygular (`providers/claude/auth.rs`).

## Codex — `GET https://chatgpt.com/backend-api/wham/usage`

- Başlıklar: `Authorization: Bearer <tokens.access_token>`,
  `ChatGPT-Account-Id: <tokens.account_id>`. 200 döndü (özel User-Agent gerekmedi).
- `rate_limit.{primary_window, secondary_window}: {used_percent,
  limit_window_seconds, reset_after_seconds, reset_at}`. Free planda tek pencere:
  30 gün (2 592 000 sn). Plus'ta beklenen: 5 saat + haftalık.
- Ek: `code_review_rate_limit`, `additional_rate_limits`, `credits`, `plan_type`.
- `~/.codex/auth.json` erişim token'ı JWT, **10 gün** geçerli; Codex kendisi yeniler.

## Yerel loglar

- Claude: `~/.claude/projects/<proje>/<oturum>.jsonl`, `type: "assistant"` satırları
  `message.{id, model, usage{input_tokens, output_tokens,
  cache_creation_input_tokens, cache_read_input_tokens}}`. Aynı mesaj birden çok
  satıra yazılabiliyor → `message.id` ile tekilleştirme. `<synthetic>` modeller hata.
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, `event_msg/token_count`:
  kümülatif `info.total_token_usage` ve `rate_limits.{primary, secondary}:
  {used_percent, window_minutes, resets_at}`. Limit bitince sonraki olaylarda
  `primary: null` geliyor; son dolu olan "son bilinen değer" sayılır.

## Bu makinede olup henüz kullanılmayanlar (Aşama 3)

- `gemini` CLI (`/usr/local/bin/gemini`) ve `~/.gemini/antigravity*` dizinleri var;
  `~/.gemini/oauth_creds.json` var (ilk taramada gözden kaçmıştı); bkz. aşağıdaki Gemini notu.
- `codex` CLI PATH'te yok (giriş dosyası Codex uygulamasından); "Bağlan" düğmesi
  bu durumda "`codex` bulunamadı" hatası verir.

## Ek doğrulamalar — 7 Kasım 2026

- Geçersiz anahtarla yoklama: DeepSeek `/user/balance`, OpenRouter `/api/v1/key` ve
  `/api/v1/credits`, Moonshot `/v1/users/me/balance`, SiliconFlow `/v1/user/info`,
  OpenAI `/v1/organization/costs`, Anthropic `/v1/organizations/cost_report`,
  GitHub `/copilot_internal/user`, Google `v1internal:retrieveUserQuota` → hepsi 401.
- Z.ai `/api/monitor/usage/quota/limit` hatada da HTTP 200 + `{code:401, success:false}` döner.
- Gemini: bu hesabın `loadCodeAssist` yanıtında `currentTier` yok; `ineligibleTiers`
  "UNSUPPORTED_CLIENT … migrate to Antigravity" diyor → Kota "hesap uygun değil" gösterir.
- Gemini CLI OAuth istemcisi ve `retrieveUserQuota` akışı paket içinden
  (`@google/gemini-cli` bundle) okundu.
- Anahtarlı sağlayıcıların başarılı yanıt şemaları belgelerden; gerçek anahtar
  olmadığı için canlı doğrulanmadı.
