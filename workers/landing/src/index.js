export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const path = url.pathname;

    // Route WebFinger requests to webfinger worker
    if (path.startsWith('/.well-known/webfinger')) {
      const webfingerUrl = env.WEBFINGER_URL + path + url.search;
      const response = await fetch(new Request(webfingerUrl, {
        method: request.method,
        headers: request.headers,
      }));

      // Preserve all headers from the webfinger worker response
      return response;
    }

    // Serve the landing page HTML
    const html = `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>dais - Self-Hosted ActivityPub on Cloudflare Workers</title>
    <meta name="description" content="A self-hosted, single-user ActivityPub server running on Cloudflare Workers. Own your social media presence.">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Helvetica Neue', Arial, sans-serif;
            background: #FFFFFF;
            color: #B45309;
            line-height: 1.6;
            min-height: 100vh;
            padding: 20px;
        }

        .container {
            max-width: 900px;
            margin: 0 auto;
            padding: 60px 0;
        }

        header {
            text-align: center;
            margin-bottom: 80px;
        }

        .logo {
            display: inline-block;
            width: 80px;
            height: 80px;
            background: linear-gradient(135deg, #FDBA74 0%, #FB923C 100%);
            border-radius: 20px;
            margin-bottom: 24px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 40px;
            color: white;
            font-weight: 700;
        }

        h1 {
            font-size: 64px;
            font-weight: 700;
            margin-bottom: 16px;
            color: #B45309;
            letter-spacing: -0.02em;
        }

        .tagline {
            font-size: 24px;
            color: #B45309;
            font-weight: 400;
            margin-bottom: 32px;
        }

        .description {
            font-size: 18px;
            color: #B45309;
            max-width: 600px;
            margin: 0 auto 48px;
            line-height: 1.8;
        }

        .hero-section {
            background: #FFFAF5;
            border-radius: 16px;
            padding: 48px;
            margin-bottom: 48px;
        }

        .features {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
            gap: 24px;
            margin: 48px 0;
        }

        .feature {
            background: #FFFAF5;
            padding: 32px;
            border-radius: 12px;
            border-left: 4px solid #FDBA74;
        }

        .feature h3 {
            font-size: 20px;
            color: #B45309;
            margin-bottom: 12px;
            font-weight: 600;
        }

        .feature p {
            color: #B45309;
            font-size: 16px;
            line-height: 1.6;
        }

        .highlight-list {
            background: #FFFAF5;
            padding: 40px;
            border-radius: 16px;
            margin: 48px 0;
        }

        .highlight-list h2 {
            font-size: 28px;
            margin-bottom: 24px;
            color: #B45309;
        }

        .highlight-item {
            padding: 16px 0;
            padding-left: 32px;
            position: relative;
            color: #B45309;
            font-size: 17px;
        }

        .highlight-item:before {
            content: "✓";
            position: absolute;
            left: 0;
            color: #FDBA74;
            font-weight: 700;
            font-size: 20px;
        }

        .buttons {
            display: flex;
            gap: 16px;
            justify-content: center;
            flex-wrap: wrap;
            margin: 48px 0;
        }

        .button {
            display: inline-block;
            padding: 16px 32px;
            border-radius: 8px;
            font-weight: 600;
            font-size: 16px;
            text-decoration: none;
            transition: all 0.2s ease;
        }

        .button-primary {
            background: #FDBA74;
            color: white;
        }

        .button-primary:hover {
            background: #FB923C;
            transform: translateY(-1px);
        }

        .button-secondary {
            background: #FFFAF5;
            color: #B45309;
            border: 1px solid #FEE2C1;
        }

        .button-secondary:hover {
            border-color: #FDBA74;
            color: #FB923C;
        }

        footer {
            text-align: center;
            margin-top: 80px;
            padding-top: 40px;
            border: 1px solid #FEE2C1;
            color: #B45309;
            font-size: 15px;
        }

        footer a {
            color: #FB923C;
            text-decoration: none;
            font-weight: 500;
        }

        footer a:hover {
            text-decoration: underline;
        }

        .status-badge {
            display: inline-block;
            background: #FEF3C7;
            color: #B45309;
            padding: 6px 12px;
            border-radius: 20px;
            font-size: 14px;
            font-weight: 600;
            margin-bottom: 24px;
        }

        @media (max-width: 768px) {
            h1 {
                font-size: 48px;
            }
            .tagline {
                font-size: 20px;
            }
            .hero-section, .highlight-list {
                padding: 32px 24px;
            }
            .feature {
                padding: 24px;
            }
            .buttons {
                flex-direction: column;
                align-items: stretch;
            }
            .button {
                text-align: center;
            }
        }

        @media (prefers-color-scheme: dark) {
            body {
                background: #1C1917;
                color: #FFFFFF;
            }
            h1, .feature h3, .highlight-list h2 {
                color: #FFFAF5;
            }
            .tagline, .description, .feature p, .highlight-item {
                color: #E7E5E4;
            }
            .hero-section, .feature, .highlight-list {
                background: #292524;
            }
            .button-secondary {
                background: #292524;
                color: #FFFAF5;
                border-color: #44403C;
            }
            .button-secondary:hover {
                border-color: #FCD34D;
                color: #FCD34D;
            }
            .button-primary {
                background: #FCD34D;
                color: #1C1917;
            }
            .button-primary:hover {
                background: #FDE68A;
            }
            footer {
                border-top-color: #44403C;
                color: #E7E5E4;
            }
            footer a {
                color: #FDE68A;
            }
        }
    </style>
</head>
<body>
    <div class="container">
        <header>
            <div class="logo">d</div>
            <span class="status-badge">v1.0.0 • Stable Release</span>
            <h1>dais</h1>
            <div class="tagline">ActivityPub + Bluesky on Cloudflare</div>
            <p class="description">
                A complete single-user social media server supporting both ActivityPub (Mastodon)
                and AT Protocol (Bluesky). Run your own instance at @you@yourdomain.com with
                zero hosting costs on Cloudflare's free tier.
            </p>
            <div class="buttons">
                <a href="https://github.com/marctjones/dais/releases/tag/v1.0.0" class="button button-primary">Download v1.0.0</a>
                <a href="https://github.com/marctjones/dais" class="button button-secondary">View on GitHub</a>
                <a href="https://social.dais.social/users/social" class="button button-secondary">Live Demo</a>
            </div>
        </header>

        <div class="features">
            <div class="feature">
                <h3>🌐 Dual Protocol Support</h3>
                <p>Full ActivityPub and AT Protocol. Post to Mastodon, Pleroma, Pixelfed, and Bluesky from one account.</p>
            </div>
            <div class="feature">
                <h3>🖥️ Terminal UI</h3>
                <p>Interactive dashboard with 6 views for managing followers, posts, notifications, DMs, and analytics in real-time.</p>
            </div>
            <div class="feature">
                <h3>🔒 Enterprise Auth</h3>
                <p>Cloudflare Access with SSO support. Google, GitHub, Microsoft identity providers. Service tokens for API access.</p>
            </div>
            <div class="feature">
                <h3>🚀 One-Command Deploy</h3>
                <p>Run "dais deploy all" to set up everything: D1 database, R2 storage, 9 Workers, migrations, and secrets.</p>
            </div>
            <div class="feature">
                <h3>⚡ Rust + WASM</h3>
                <p>9 Workers compiled to WebAssembly for maximum performance. Global edge deployment in 300+ locations.</p>
            </div>
            <div class="feature">
                <h3>💰 Zero Cost Hosting</h3>
                <p>Runs entirely on Cloudflare free tier. $0/month for typical use. Unlimited bandwidth, no egress fees.</p>
            </div>
        </div>

        <div class="highlight-list">
            <h2>What's in v1.0.0?</h2>
            <div class="highlight-item">200+ features including posts, replies, likes, boosts, and direct messages</div>
            <div class="highlight-item">Terminal UI with 6 interactive views for complete management</div>
            <div class="highlight-item">Full CLI with 14 command groups (post, followers, search, notifications, etc.)</div>
            <div class="highlight-item">Cloudflare Access authentication with multiple identity providers</div>
            <div class="highlight-item">Media attachments (images, videos) via R2 object storage</div>
            <div class="highlight-item">Search users and posts across the Fediverse</div>
            <div class="highlight-item">Moderation tools: block accounts and instances</div>
            <div class="highlight-item">Backup and restore for complete data safety</div>
            <div class="highlight-item">Real-time notifications and statistics dashboard</div>
            <div class="highlight-item">12 comprehensive documentation guides</div>
        </div>

        <footer>
            <p>🎉 v1.0.0 Stable Release - Own your social media presence on your own terms.</p>
            <p style="margin-top: 16px;">
                Follow the project: <a href="https://social.dais.social/users/social">@social@dais.social</a> •
                <a href="https://bsky.app/profile/social.dais.social">@social.dais.social on Bluesky</a>
            </p>
            <p style="margin-top: 16px;">
                <a href="https://github.com/marctjones/dais/releases/tag/v1.0.0">Release Notes</a> •
                <a href="https://github.com/marctjones/dais/blob/main/INSTALL.md">Installation Guide</a> •
                <a href="https://github.com/marctjones/dais/blob/main/FEATURES.md">Feature List</a>
            </p>
            <p style="margin-top: 24px; font-size: 14px; opacity: 0.8;">
                Built with ❤️ by the dais community • <a href="https://github.com/marctjones/dais">Open Source (MIT License)</a>
            </p>
        </footer>
    </div>
</body>
</html>`;

    return new Response(html, {
      headers: {
        'Content-Type': 'text/html;charset=UTF-8',
        'Cache-Control': 'public, max-age=3600',
      },
    });
  },
};
