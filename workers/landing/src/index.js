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
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: #ffffff;
            line-height: 1.6;
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 20px;
        }
        .container {
            max-width: 800px;
            background: rgba(255, 255, 255, 0.1);
            backdrop-filter: blur(10px);
            border-radius: 20px;
            padding: 60px 40px;
            box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
            border: 1px solid rgba(255, 255, 255, 0.2);
        }
        h1 {
            font-size: 72px;
            font-weight: 700;
            margin-bottom: 20px;
            text-shadow: 2px 2px 4px rgba(0, 0, 0, 0.2);
        }
        .tagline {
            font-size: 24px;
            margin-bottom: 40px;
            opacity: 0.95;
        }
        .description {
            font-size: 18px;
            margin-bottom: 40px;
            line-height: 1.8;
            opacity: 0.9;
        }
        .features {
            margin: 40px 0;
        }
        .feature {
            margin: 20px 0;
            padding-left: 30px;
            position: relative;
        }
        .feature:before {
            content: "✓";
            position: absolute;
            left: 0;
            font-weight: bold;
            font-size: 20px;
        }
        .buttons {
            display: flex;
            gap: 20px;
            flex-wrap: wrap;
            margin-top: 40px;
        }
        .button {
            display: inline-block;
            padding: 15px 30px;
            background: rgba(255, 255, 255, 0.9);
            color: #764ba2;
            text-decoration: none;
            border-radius: 8px;
            font-weight: 600;
            transition: all 0.3s ease;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
        }
        .button:hover {
            background: rgba(255, 255, 255, 1);
            transform: translateY(-2px);
            box-shadow: 0 6px 12px rgba(0, 0, 0, 0.2);
        }
        .button.secondary {
            background: rgba(255, 255, 255, 0.2);
            color: #ffffff;
        }
        .button.secondary:hover {
            background: rgba(255, 255, 255, 0.3);
        }
        .footer {
            margin-top: 60px;
            padding-top: 30px;
            border-top: 1px solid rgba(255, 255, 255, 0.2);
            text-align: center;
            opacity: 0.8;
            font-size: 14px;
        }
        .footer a {
            color: #ffffff;
            text-decoration: underline;
        }
        @media (max-width: 600px) {
            h1 {
                font-size: 48px;
            }
            .tagline {
                font-size: 20px;
            }
            .container {
                padding: 40px 30px;
            }
            .buttons {
                flex-direction: column;
            }
            .button {
                text-align: center;
            }
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>dais</h1>
        <div class="tagline">Self-Hosted ActivityPub on Cloudflare Workers</div>

        <div class="description">
            A modern, single-user ActivityPub server that runs entirely on Cloudflare's edge network.
            Own your social media presence without managing servers.
        </div>

        <div class="features">
            <div class="feature">Built with Rust, compiled to WebAssembly</div>
            <div class="feature">Runs on Cloudflare Workers (serverless edge computing)</div>
            <div class="feature">Full ActivityPub protocol support (federate with Mastodon, etc.)</div>
            <div class="feature">Cloudflare D1 for data persistence</div>
            <div class="feature">HTTP signatures for secure federation</div>
            <div class="feature">Single-user focus - your corner of the fediverse</div>
            <div class="feature">CLI for easy post management</div>
        </div>

        <div class="buttons">
            <a href="https://github.com/marctjones/dais" class="button">View on GitHub</a>
            <a href="https://social.dais.social/users/social" class="button secondary">Demo Instance</a>
        </div>

        <div class="footer">
            <p>Join the fediverse on your own terms.</p>
            <p style="margin-top: 10px;">
                Follow the official account:
                <a href="https://social.dais.social/users/social">@social@dais.social</a>
            </p>
        </div>
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
