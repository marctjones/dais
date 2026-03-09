export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const path = url.pathname;

    // Route to appropriate worker based on path
    let targetUrl;

    if (path.startsWith('/.well-known/webfinger')) {
      targetUrl = env.WEBFINGER_URL + path + url.search;
    } else if (path.match(/^\/users\/[^\/]+\/inbox/)) {
      targetUrl = env.INBOX_URL + path + url.search;
    } else if (path.match(/^\/users\/[^\/]+\/outbox/)) {
      targetUrl = env.OUTBOX_URL + path + url.search;
    } else if (path.match(/^\/users\/[^\/]+\/posts\//)) {
      targetUrl = env.OUTBOX_URL + path + url.search;
    } else if (path.match(/^\/users\/[^\/]+/)) {
      targetUrl = env.ACTOR_URL + path + url.search;
    } else {
      return new Response('Not Found', { status: 404 });
    }

    // Proxy the request to the target worker
    const targetRequest = new Request(targetUrl, {
      method: request.method,
      headers: request.headers,
      body: request.body,
    });

    return fetch(targetRequest);
  },
};
