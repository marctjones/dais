export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);

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
            <span class="status-badge">Alpha • In Development</span>
            <h1>dais</h1>
            <div class="tagline">Self-hosted ActivityPub on Cloudflare Workers</div>
            <p class="description">
                A modern, single-user ActivityPub server that runs entirely on Cloudflare's edge network.
                Own your social media presence without managing servers.
            </p>
            <div class="buttons">
                <a href="https://github.com/marctjones/dais" class="button button-primary">View on GitHub</a>
                <a href="https://social.dais.social/users/social" class="button button-secondary">Live Demo</a>
            </div>
        </header>

        <div class="features">
            <div class="feature">
                <h3>Serverless Edge</h3>
                <p>Runs on Cloudflare Workers - no servers to manage, automatic scaling, deployed globally.</p>
            </div>
            <div class="feature">
                <h3>Open Federation</h3>
                <p>Full ActivityPub protocol support. Connect with Mastodon, Pixelfed, and the entire fediverse.</p>
            </div>
            <div class="feature">
                <h3>Built with Rust</h3>
                <p>Compiled to WebAssembly for performance and security. Type-safe, fast, reliable.</p>
            </div>
        </div>

        <div class="highlight-list">
            <h2>Why dais?</h2>
            <div class="highlight-item">Single-user focus - your corner of the fediverse</div>
            <div class="highlight-item">Cloudflare D1 for data persistence</div>
            <div class="highlight-item">HTTP signatures for secure federation</div>
            <div class="highlight-item">CLI for easy post management</div>
            <div class="highlight-item">Clean, modern HTML interface</div>
            <div class="highlight-item">Deploy in minutes with wrangler</div>
        </div>

        <footer>
            <p>Join the fediverse on your own terms.</p>
            <p style="margin-top: 16px;">
                Follow the project: <a href="https://social.dais.social/users/social">@social@dais.social</a>
            </p>
            <p style="margin-top: 24px; font-size: 14px; opacity: 0.8;">
                Built with ❤️ by the dais community • <a href="https://github.com/marctjones/dais">Open Source</a>
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
