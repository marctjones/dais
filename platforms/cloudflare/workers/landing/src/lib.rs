/// Refactored Landing worker
///
/// This worker serves the landing page for the dais instance.
/// It provides information about the instance and links to the actor profile.
use worker::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();

    match path {
        "/" => handle_landing(env).await,
        "/health" => Response::ok("OK"),
        // Email-style discovery: @user@apex-domain. The apex (dais.social) is served
        // by this worker, so proxy WebFinger to the webfinger worker so handles that
        // match the base domain resolve. (Actor still lives on the AP subdomain.)
        "/.well-known/webfinger" => proxy_to_webfinger(&url, &env).await,
        _ => Response::error("Not Found", 404),
    }
}

async fn proxy_to_webfinger(url: &Url, env: &Env) -> Result<Response> {
    let base = env
        .var("WEBFINGER_URL")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "https://webfinger-production.marc-t-jones.workers.dev".to_string());

    let mut target = format!("{}/.well-known/webfinger", base.trim_end_matches('/'));
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query);
    }

    let target_url =
        Url::parse(&target).map_err(|e| Error::from(format!("invalid WEBFINGER_URL: {e}")))?;
    Fetch::Url(target_url).send().await
}

async fn handle_landing(env: Env) -> Result<Response> {
    // Get configuration from environment
    let domain = env
        .var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());

    let activitypub_domain = env
        .var("ACTIVITYPUB_DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("social.{}", domain));

    let username = env
        .var("USERNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social".to_string());

    let actor_url = format!("https://{}/users/{}", activitypub_domain, username);

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>dais - Open source private-by-default social server</title>
    <style>
        :root {{
            color-scheme: light;
            --ink: #17211b;
            --muted: #536057;
            --line: #d8ded8;
            --panel: #f6f8f5;
            --accent: #0f766e;
            --accent-strong: #134e4a;
            --soft: #edf5f1;
            --warn: #8a5a00;
        }}
        * {{
            box-sizing: border-box;
        }}
        body {{
            font-family: system-ui, -apple-system, sans-serif;
            max-width: 1120px;
            margin: 0 auto;
            padding: 0 1.25rem 2rem;
            line-height: 1.55;
            color: var(--ink);
            background: #ffffff;
        }}
        header {{
            min-height: 72vh;
            display: grid;
            align-content: center;
            gap: 2rem;
            padding: 3rem 0 2rem;
            border-bottom: 1px solid var(--line);
        }}
        h1 {{
            margin: 0;
            font-size: clamp(3rem, 11vw, 7rem);
            line-height: 0.92;
            letter-spacing: 0;
            color: var(--accent-strong);
        }}
        h2 {{
            margin: 0 0 1rem;
            font-size: 1.35rem;
        }}
        h3 {{
            margin: 0 0 0.4rem;
            font-size: 1rem;
        }}
        p {{
            margin: 0 0 1rem;
        }}
        .lede {{
            max-width: 780px;
            font-size: clamp(1.15rem, 2vw, 1.55rem);
            color: var(--muted);
        }}
        .hero-actions, .links {{
            display: flex;
            flex-wrap: wrap;
            gap: 0.75rem;
        }}
        a {{
            color: var(--accent-strong);
            text-decoration: none;
            font-weight: 650;
        }}
        a:hover {{
            text-decoration: underline;
        }}
        .button {{
            display: inline-flex;
            align-items: center;
            min-height: 2.75rem;
            padding: 0.7rem 1rem;
            border: 1px solid var(--accent);
            background: var(--accent);
            color: #ffffff;
            border-radius: 0.4rem;
        }}
        .button.secondary {{
            background: #ffffff;
            color: var(--accent-strong);
        }}
        main {{
            display: grid;
            gap: 3rem;
            padding-top: 2rem;
        }}
        section {{
            padding: 1rem 0;
        }}
        .grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
            gap: 1rem;
        }}
        .tile {{
            border: 1px solid var(--line);
            border-radius: 0.45rem;
            padding: 1rem;
            background: var(--panel);
            min-height: 9rem;
        }}
        .tile p {{
            color: var(--muted);
        }}
        .status {{
            display: inline-block;
            margin-bottom: 0.65rem;
            padding: 0.2rem 0.5rem;
            border-radius: 999px;
            background: var(--soft);
            color: var(--accent-strong);
            font-size: 0.82rem;
            font-weight: 700;
        }}
        .status.partial {{
            background: #fff7df;
            color: var(--warn);
        }}
        .instance {{
            border-left: 4px solid var(--accent);
            padding: 1rem 0 1rem 1rem;
            background: linear-gradient(90deg, var(--soft), transparent);
        }}
        code {{
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            font-size: 0.95em;
        }}
        ul {{
            padding-left: 1.25rem;
        }}
        li {{
            margin: 0.35rem 0;
        }}
        .footer {{
            margin-top: 3rem;
            padding-top: 1rem;
            border-top: 1px solid var(--line);
            color: var(--muted);
            font-size: 0.875rem;
        }}
        @media (max-width: 720px) {{
            header {{
                min-height: auto;
                padding-top: 2rem;
            }}
            .button {{
                width: 100%;
                justify-content: center;
            }}
        }}
    </style>
</head>
<body>
    <header>
        <div>
            <h1>dais</h1>
            <p class="lede">An open source, private-by-default social server for Cloudflare. dais is a <a href="https://skpt.cl">Skeptical Engineering</a> project: run your own domain, post privately to approved followers, publish publicly to the wider fediverse, and bridge public posts toward Bluesky / AT Protocol.</p>
        </div>
        <div class="hero-actions">
            <a class="button" href="https://github.com/marctjones/dais">View the source</a>
            <a class="button secondary" href="{actor_url}">Open the project instance</a>
            <a class="button secondary" href="https://pds.{domain}/xrpc/com.atproto.server.describeServer">Inspect the PDS</a>
        </div>
    </header>

    <main>
        <section class="instance">
            <h2>The live dais project instance</h2>
            <p>This is the project account for dais by <a href="https://skpt.cl">Skeptical Engineering</a>, not a personal account. The separate Skeptical Engineering test instance now runs independently at <a href="https://social.skpt.cl/users/social"><code>@social@skpt.cl</code></a>; this instance remains the live project/demo presence for dais itself.</p>
            <p><strong>ActivityPub handle:</strong> <code>@{username}@{domain}</code></p>
            <p><strong>ActivityPub profile:</strong> <a href="{actor_url}">{actor_url}</a></p>
            <p><strong>ActivityPub origin:</strong> <a href="https://{activitypub_domain}">https://{activitypub_domain}</a></p>
            <p><strong>AT Protocol PDS origin:</strong> <a href="https://pds.{domain}">https://pds.{domain}</a></p>
        </section>

        <section>
            <h2>What dais is for</h2>
            <div class="grid">
                <div class="tile">
                    <span class="status">vision</span>
                    <h3>Own your social graph</h3>
                    <p>dais is built for people, families, communities, and small organizations that want a social presence on their own domain instead of inside a platform silo.</p>
                </div>
                <div class="tile">
                    <span class="status">default</span>
                    <h3>Private first</h3>
                    <p>Rust CLI and TUI posts default to followers-only. Public broadcast is an explicit choice, and non-public content is not routed to Bluesky.</p>
                </div>
                <div class="tile">
                    <span class="status">open</span>
                    <h3>Federated by design</h3>
                    <p>ActivityPub is the primary federation layer for public and private/followers posts. AT Protocol support provides a growing public-read and public-posting surface.</p>
                </div>
            </div>
        </section>

        <section>
            <h2>Implementation status</h2>
            <div class="grid">
                <div class="tile">
                    <span class="status">production</span>
                    <h3>ActivityPub foundation</h3>
                    <p>WebFinger, actor, inbox, outbox, public dereference, delivery queueing, HTTP signatures, locked-profile signaling, private/E2EE anonymous denial, and cross-instance reply threading are implemented.</p>
                </div>
                <div class="tile">
                    <span class="status">production</span>
                    <h3>Rust operator client</h3>
                    <p>The owner interface is the Rust CLI/TUI and native Dais Desk app. It manages posts, timelines, followers, notifications, deliveries, E2EE helpers, events, actor mode, and diagnostics.</p>
                </div>
                <div class="tile">
                    <span class="status partial">partial</span>
                    <h3>Mastodon compatibility</h3>
                    <p>dais is Mastodon-readable and exposes a read-oriented Mastodon API floor. It is not a full Mastodon server replacement yet.</p>
                </div>
                <div class="tile">
                    <span class="status partial">partial</span>
                    <h3>Bluesky / AT Protocol</h3>
                    <p>The PDS exposes identity, repo metadata, public record reads, author feed, timeline, notifications, likes, followers, follows, and subscribeRepos status. Full AT Protocol compatibility remains in progress.</p>
                </div>
                <div class="tile">
                    <span class="status">verified</span>
                    <h3>E2EE</h3>
                    <p>The encryptedMessage v1 fallback and MLS v2 owner-device paths are implemented. The independent skpt.cl deployment passed the 2026-07-04 strict production release gate with dais.social: bidirectional owner-DM delivery/decrypt, MLS direct messages, audience-list group delivery/decrypt, two-device recipient decrypt, removed-device decrypt failure, and delivery-worker processing.</p>
                </div>
                <div class="tile">
                    <span class="status partial">in progress</span>
                    <h3>Media and polish</h3>
                    <p>R2 media serving, private ActivityPub media, AT Protocol public image upload, and encrypted media attachments are implemented. Desk media presentation, profile editing, moderation UI, and analytics remain active roadmap items.</p>
                </div>
            </div>
        </section>

        <section>
            <h2>Use it or follow along</h2>
            <ul>
                <li><a href="https://skpt.cl/projects/dais/">dais on Skeptical Engineering</a></li>
                <li><a href="https://social.skpt.cl/users/social">Independent skpt.cl dais instance</a></li>
                <li><a href="https://github.com/marctjones/dais">Current public source repository and issues</a></li>
                <li><a href="https://github.com/marctjones/dais/blob/main/README.md">README and setup notes</a></li>
                <li><a href="https://github.com/marctjones/dais/blob/main/docs/POSITIONING.md">Positioning and product vision</a></li>
                <li><a href="{actor_url}">Follow the live dais project instance</a></li>
            </ul>
        </section>
    </main>

    <div class="footer">
        <p>Open source under active development. Running on Cloudflare Workers, D1, R2, and Queues. Homepage status updated after the 2026-07-04 strict production/skpt server release gate.</p>
    </div>
</body>
</html>"#,
        domain = domain,
        activitypub_domain = activitypub_domain,
        username = username,
        actor_url = actor_url
    );

    Response::from_html(html)
}
