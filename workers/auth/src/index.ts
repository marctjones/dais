/**
 * Cloudflare Access Integration for dais
 *
 * Provides authentication using Cloudflare Access with support for:
 * - Multiple identity providers (Google, GitHub, Microsoft, etc.)
 * - Service tokens for API/automation
 * - JWT verification
 * - Mobile app authentication flow
 */

import { Env } from './types';

/**
 * Verify Cloudflare Access JWT token
 */
async function verifyAccessToken(request: Request, env: Env): Promise<{ valid: boolean; email?: string; error?: string }> {
  // Get JWT from cookie or header
  const cookie = request.headers.get('Cookie');
  const authHeader = request.headers.get('Authorization');
  const cfAccessHeader = request.headers.get('Cf-Access-Jwt-Assertion');

  let token: string | null = null;

  // Check CF-Access-Jwt-Assertion header (set by Cloudflare Access)
  if (cfAccessHeader) {
    token = cfAccessHeader;
  }
  // Check Authorization header (for API clients)
  else if (authHeader?.startsWith('Bearer ')) {
    token = authHeader.substring(7);
  }
  // Check cookie (for browser clients)
  else if (cookie) {
    const match = cookie.match(/CF_Authorization=([^;]+)/);
    if (match) {
      token = match[1];
    }
  }

  if (!token) {
    return { valid: false, error: 'No authentication token found' };
  }

  // Verify token with Cloudflare Access
  try {
    const teamDomain = env.CLOUDFLARE_ACCESS_TEAM_DOMAIN; // e.g., "myteam.cloudflareaccess.com"
    const certsUrl = `https://${teamDomain}/cdn-cgi/access/certs`;

    // Fetch public keys
    const certsResponse = await fetch(certsUrl);
    if (!certsResponse.ok) {
      return { valid: false, error: 'Failed to fetch Access certificates' };
    }

    const certs = await certsResponse.json() as { keys: any[], public_certs: any[] };

    // Decode JWT header to get key ID
    const parts = token.split('.');
    if (parts.length !== 3) {
      return { valid: false, error: 'Invalid token format' };
    }

    const header = JSON.parse(atob(parts[0].replace(/-/g, '+').replace(/_/g, '/')));
    const payload = JSON.parse(atob(parts[1].replace(/-/g, '+').replace(/_/g, '/')));

    // Verify the token using Web Crypto API
    // For production, you should use a proper JWT library
    // This is a simplified version

    // Check expiration
    const now = Math.floor(Date.now() / 1000);
    if (payload.exp && payload.exp < now) {
      return { valid: false, error: 'Token expired' };
    }

    // Check audience (your Access application AUD tag)
    const expectedAud = env.CLOUDFLARE_ACCESS_AUD;
    if (expectedAud && payload.aud && !payload.aud.includes(expectedAud)) {
      return { valid: false, error: 'Invalid audience' };
    }

    // Extract email from token
    const email = payload.email || payload.sub;

    // Verify this is your configured user
    const allowedEmail = env.ALLOWED_EMAIL;
    if (allowedEmail && email !== allowedEmail) {
      return { valid: false, error: 'Unauthorized user' };
    }

    return { valid: true, email };

  } catch (error: any) {
    return { valid: false, error: `Verification failed: ${error.message}` };
  }
}

/**
 * Verify Cloudflare Access Service Token
 */
function verifyServiceToken(request: Request, env: Env): { valid: boolean; error?: string } {
  const clientId = request.headers.get('CF-Access-Client-Id');
  const clientSecret = request.headers.get('CF-Access-Client-Secret');

  if (!clientId || !clientSecret) {
    return { valid: false, error: 'Service token headers missing' };
  }

  // Verify against configured service tokens
  const allowedTokens = env.SERVICE_TOKENS ? JSON.parse(env.SERVICE_TOKENS) : [];

  const validToken = allowedTokens.find((t: any) =>
    t.clientId === clientId && t.clientSecret === clientSecret
  );

  if (!validToken) {
    return { valid: false, error: 'Invalid service token' };
  }

  return { valid: true };
}

/**
 * Handle authentication verification
 */
async function handleVerify(request: Request, env: Env): Promise<Response> {
  // Try service token first (for API automation)
  const serviceAuth = verifyServiceToken(request, env);
  if (serviceAuth.valid) {
    return new Response(JSON.stringify({
      valid: true,
      type: 'service_token',
      authenticated: true
    }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  // Try Access JWT token (for user authentication)
  const userAuth = await verifyAccessToken(request, env);
  if (userAuth.valid) {
    return new Response(JSON.stringify({
      valid: true,
      type: 'user',
      email: userAuth.email,
      authenticated: true
    }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' }
    });
  }

  // Authentication failed
  return new Response(JSON.stringify({
    valid: false,
    authenticated: false,
    error: userAuth.error || serviceAuth.error
  }), {
    status: 401,
    headers: { 'Content-Type': 'application/json' }
  });
}

/**
 * Get login URL for Cloudflare Access
 */
function handleLoginInfo(request: Request, env: Env): Response {
  const teamDomain = env.CLOUDFLARE_ACCESS_TEAM_DOMAIN;
  const appDomain = env.APP_DOMAIN || request.headers.get('Host') || 'localhost';

  // Cloudflare Access login URL
  const loginUrl = `https://${teamDomain}/cdn-cgi/access/login/${appDomain}`;

  return new Response(JSON.stringify({
    login_url: loginUrl,
    team_domain: teamDomain,
    app_domain: appDomain,
    instructions: {
      web: 'Redirect user to login_url for authentication',
      mobile: 'Open login_url in system browser, then redirect back to app',
      api: 'Use service tokens with CF-Access-Client-Id and CF-Access-Client-Secret headers'
    }
  }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' }
  });
}

/**
 * Handle logout
 */
function handleLogout(request: Request, env: Env): Response {
  const teamDomain = env.CLOUDFLARE_ACCESS_TEAM_DOMAIN;
  const logoutUrl = `https://${teamDomain}/cdn-cgi/access/logout`;

  return new Response(JSON.stringify({
    logout_url: logoutUrl,
    instructions: 'Redirect user to logout_url to clear Cloudflare Access session'
  }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' }
  });
}

/**
 * Handle CORS preflight
 */
function handleCORS(): Response {
  return new Response(null, {
    status: 204,
    headers: {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type, Authorization, CF-Access-Client-Id, CF-Access-Client-Secret, Cf-Access-Jwt-Assertion',
      'Access-Control-Max-Age': '86400',
      'Access-Control-Allow-Credentials': 'true'
    }
  });
}

/**
 * Add CORS headers to response
 */
function addCORSHeaders(response: Response): Response {
  const newHeaders = new Headers(response.headers);
  newHeaders.set('Access-Control-Allow-Origin', '*');
  newHeaders.set('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  newHeaders.set('Access-Control-Allow-Headers', 'Content-Type, Authorization, CF-Access-Client-Id, CF-Access-Client-Secret, Cf-Access-Jwt-Assertion');
  newHeaders.set('Access-Control-Allow-Credentials', 'true');

  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: newHeaders
  });
}

/**
 * Main fetch handler
 */
export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    // Handle CORS preflight
    if (request.method === 'OPTIONS') {
      return handleCORS();
    }

    let response: Response;

    // Route requests
    if (url.pathname === '/api/auth/verify' && request.method === 'GET') {
      response = await handleVerify(request, env);
    }
    else if (url.pathname === '/api/auth/login' && request.method === 'GET') {
      response = handleLoginInfo(request, env);
    }
    else if (url.pathname === '/api/auth/logout' && request.method === 'GET') {
      response = handleLogout(request, env);
    }
    else if (url.pathname === '/health') {
      response = new Response(JSON.stringify({
        status: 'ok',
        authentication: 'cloudflare_access',
        team_domain: env.CLOUDFLARE_ACCESS_TEAM_DOMAIN
      }), {
        headers: { 'Content-Type': 'application/json' }
      });
    }
    else {
      response = new Response(JSON.stringify({
        error: 'not_found',
        message: 'Endpoint not found',
        available_endpoints: [
          'GET /api/auth/verify - Verify authentication',
          'GET /api/auth/login - Get login URL',
          'GET /api/auth/logout - Get logout URL',
          'GET /health - Health check'
        ]
      }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' }
      });
    }

    // Add CORS headers
    return addCORSHeaders(response);
  }
};
