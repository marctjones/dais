

# Authentication API - Cloudflare Access Integration

This document describes how to use **Cloudflare Access** for authentication with your dais instance. Cloudflare Access is a built-in authentication product that handles all authentication logic, supports multiple identity providers, and provides enterprise-grade security.

## Overview

dais uses **Cloudflare Access** (part of Cloudflare Zero Trust) for authentication. This provides:

- ✅ **Multiple Identity Providers** - Google, GitHub, Microsoft, Facebook, LinkedIn, etc.
- ✅ **No Password Management** - Cloudflare handles all authentication
- ✅ **Built-in Security** - Enterprise-grade authentication and authorization
- ✅ **Single Sign-On (SSO)** - One login for all your apps
- ✅ **Service Tokens** - API access for automation and scripts
- ✅ **Global Performance** - Fast authentication via Cloudflare's edge network
- ✅ **Free Tier Available** - Up to 50 users free

---

## Quick Start

### 1. Set Up Cloudflare Access

```bash
dais auth setup
```

This interactive command will guide you through:
1. Creating a Cloudflare Zero Trust account
2. Setting up an Access application
3. Configuring an identity provider (Google, GitHub, etc.)
4. Creating an access policy
5. Uploading configuration to your auth worker

### 2. Deploy Auth Worker

```bash
dais deploy workers
```

### 3. Test Authentication

```bash
dais auth test
```

### 4. Try It Out

Open your browser and visit:
```
https://yourdomain.com/api/auth/login
```

You'll be redirected to Cloudflare Access login, where you can authenticate with your chosen provider.

---

## Authentication Flows

### Browser-Based Authentication (Web/Desktop Apps)

```
┌─────────┐              ┌──────────────┐              ┌──────────┐
│  User   │              │  Cloudflare  │              │   dais   │
│ Browser │              │    Access    │              │    API   │
└────┬────┘              └──────┬───────┘              └────┬─────┘
     │                          │                           │
     │ 1. Visit protected page  │                           │
     │─────────────────────────────────────────────────────>│
     │                          │                           │
     │ 2. Redirect to login     │                           │
     │<────────────────────────────────────────────────────│
     │                          │                           │
     │ 3. Show login page       │                           │
     │<─────────────────────────│                           │
     │                          │                           │
     │ 4. Authenticate with     │                           │
     │    Google/GitHub/etc     │                           │
     │─────────────────────────>│                           │
     │                          │                           │
     │ 5. Set CF_Authorization  │                           │
     │    cookie                │                           │
     │<─────────────────────────│                           │
     │                          │                           │
     │ 6. Access protected page │                           │
     │    (with cookie)         │                           │
     │─────────────────────────────────────────────────────>│
     │                          │                           │
     │ 7. Cloudflare verifies   │                           │
     │    authentication        │                           │
     │                          │                           │
     │ 8. Request forwarded     │                           │
     │    with JWT header       │                           │
     │                          │                           │
     │ 9. Response returned     │                           │
     │<─────────────────────────────────────────────────────│
```

### Mobile App Authentication

Mobile apps should use the **system browser** for authentication (OAuth 2.0 PKCE flow):

```
┌──────────┐         ┌─────────┐         ┌──────────────┐
│  Mobile  │         │ Browser │         │  Cloudflare  │
│   App    │         │         │         │    Access    │
└────┬─────┘         └────┬────┘         └──────┬───────┘
     │                    │                      │
     │ 1. Open login URL  │                      │
     │    in browser      │                      │
     │───────────────────>│                      │
     │                    │                      │
     │                    │ 2. Load login page   │
     │                    │─────────────────────>│
     │                    │                      │
     │                    │ 3. User authenticates│
     │                    │─────────────────────>│
     │                    │                      │
     │                    │ 4. Redirect with     │
     │                    │    callback URL      │
     │                    │<─────────────────────│
     │                    │                      │
     │ 5. App receives    │                      │
     │    callback with   │                      │
     │    token           │                      │
     │<───────────────────│                      │
     │                    │                      │
     │ 6. Store token,    │                      │
     │    close browser   │                      │
     │                    │                      │
```

### API/Automation Authentication (Service Tokens)

For automation, scripts, and CI/CD:

```
┌────────────┐              ┌──────────┐
│  Script/   │              │   dais   │
│  CI/CD     │              │    API   │
└─────┬──────┘              └────┬─────┘
      │                          │
      │ 1. API request with      │
      │    service token headers │
      │─────────────────────────>│
      │                          │
      │ CF-Access-Client-Id      │
      │ CF-Access-Client-Secret  │
      │                          │
      │ 2. Cloudflare verifies   │
      │    service token         │
      │                          │
      │ 3. Response returned     │
      │<─────────────────────────│
```

---

## API Endpoints

All endpoints are automatically protected by Cloudflare Access.

### GET /api/auth/login

Get the Cloudflare Access login URL.

**Request:**
```bash
curl https://yourdomain.com/api/auth/login
```

**Response:**
```json
{
  "login_url": "https://myteam.cloudflareaccess.com/cdn-cgi/access/login/yourdomain.com",
  "team_domain": "myteam.cloudflareaccess.com",
  "app_domain": "yourdomain.com",
  "instructions": {
    "web": "Redirect user to login_url for authentication",
    "mobile": "Open login_url in system browser, then redirect back to app",
    "api": "Use service tokens with CF-Access-Client-Id and CF-Access-Client-Secret headers"
  }
}
```

### GET /api/auth/verify

Verify current authentication status.

**Request (with authentication):**
```bash
curl https://yourdomain.com/api/auth/verify \
  -H "Cookie: CF_Authorization=YOUR_TOKEN"
```

**Response (authenticated user):**
```json
{
  "valid": true,
  "type": "user",
  "email": "you@example.com",
  "authenticated": true
}
```

**Response (service token):**
```json
{
  "valid": true,
  "type": "service_token",
  "authenticated": true
}
```

**Response (not authenticated):**
```json
{
  "valid": false,
  "authenticated": false,
  "error": "No authentication token found"
}
```

### GET /api/auth/logout

Get the Cloudflare Access logout URL.

**Request:**
```bash
curl https://yourdomain.com/api/auth/logout
```

**Response:**
```json
{
  "logout_url": "https://myteam.cloudflareaccess.com/cdn-cgi/access/logout",
  "instructions": "Redirect user to logout_url to clear Cloudflare Access session"
}
```

---

## Client Implementation

### JavaScript/TypeScript (Web/Desktop)

```typescript
class CloudflareAccessAuth {
  private teamDomain: string;
  private appDomain: string;
  private loginUrl: string;

  constructor(appDomain: string) {
    this.appDomain = appDomain;
  }

  async initialize(): Promise<void> {
    // Get login info from API
    const response = await fetch(`https://${this.appDomain}/api/auth/login`);
    const data = await response.json();

    this.teamDomain = data.team_domain;
    this.loginUrl = data.login_url;
  }

  /**
   * Redirect user to Cloudflare Access login
   */
  login(): void {
    window.location.href = this.loginUrl;
  }

  /**
   * Check if user is authenticated
   */
  async isAuthenticated(): Promise<boolean> {
    try {
      const response = await fetch(`https://${this.appDomain}/api/auth/verify`, {
        credentials: 'include' // Include cookies
      });

      if (response.ok) {
        const data = await response.json();
        return data.authenticated === true;
      }

      return false;
    } catch {
      return false;
    }
  }

  /**
   * Make authenticated API request
   */
  async makeRequest(url: string, options: RequestInit = {}): Promise<Response> {
    // Cookies are sent automatically with credentials: 'include'
    return fetch(url, {
      ...options,
      credentials: 'include'
    });
  }

  /**
   * Logout user
   */
  async logout(): Promise<void> {
    const response = await fetch(`https://${this.appDomain}/api/auth/logout`);
    const data = await response.json();

    // Redirect to logout URL
    window.location.href = data.logout_url;
  }
}

// Usage
const auth = new CloudflareAccessAuth('yourdomain.com');
await auth.initialize();

// Check if authenticated
if (!await auth.isAuthenticated()) {
  // Redirect to login
  auth.login();
}

// Make authenticated requests
const response = await auth.makeRequest('https://yourdomain.com/api/posts');
const posts = await response.json();
```

### Python (Desktop/Scripts)

```python
import requests
import webbrowser

class CloudflareAccessAuth:
    def __init__(self, app_domain: str):
        self.app_domain = app_domain
        self.session = requests.Session()
        self._initialize()

    def _initialize(self):
        """Get login information"""
        response = self.session.get(f"https://{self.app_domain}/api/auth/login")
        data = response.json()
        self.login_url = data["login_url"]
        self.team_domain = data["team_domain"]

    def login(self):
        """Open browser for authentication"""
        print(f"Opening browser for authentication...")
        print(f"Please login at: {self.login_url}")
        webbrowser.open(self.login_url)

        input("\nPress Enter after you've logged in...")

        # Verify authentication
        if self.is_authenticated():
            print("✓ Authentication successful!")
        else:
            print("✗ Authentication failed. Please try again.")

    def is_authenticated(self) -> bool:
        """Check if authenticated"""
        try:
            response = self.session.get(
                f"https://{self.app_domain}/api/auth/verify"
            )
            if response.ok:
                data = response.json()
                return data.get("authenticated", False)
        except:
            pass
        return False

    def make_request(self, method: str, url: str, **kwargs) -> requests.Response:
        """Make authenticated request"""
        return self.session.request(method, url, **kwargs)

# Usage
auth = CloudflareAccessAuth("yourdomain.com")

# Login (opens browser)
if not auth.is_authenticated():
    auth.login()

# Make authenticated requests
response = auth.make_request("GET", "https://yourdomain.com/api/posts")
posts = response.json()
```

### Swift (iOS/macOS)

```swift
import Foundation
import AuthenticationServices

class CloudflareAccessAuth: NSObject, ASWebAuthenticationPresentationContextProviding {
    private let appDomain: String
    private var loginUrl: String?
    private var logoutUrl: String?

    init(appDomain: String) {
        self.appDomain = appDomain
        super.init()
    }

    func initialize() async throws {
        let url = URL(string: "https://\(appDomain)/api/auth/login")!
        let (data, _) = try await URLSession.shared.data(from: url)
        let response = try JSONDecoder().decode(LoginInfoResponse.self, from: data)

        self.loginUrl = response.loginUrl
    }

    func login() async throws {
        guard let loginUrl = self.loginUrl else {
            throw AuthError.notInitialized
        }

        // Use ASWebAuthenticationSession for OAuth-like flow
        let callbackURLScheme = "dais" // Your app's URL scheme

        return try await withCheckedThrowingContinuation { continuation in
            let session = ASWebAuthenticationSession(
                url: URL(string: loginUrl)!,
                callbackURLScheme: callbackURLScheme
            ) { callbackURL, error in
                if let error = error {
                    continuation.resume(throwing: error)
                    return
                }

                // Authentication successful
                // Cloudflare Access sets cookies automatically
                continuation.resume()
            }

            session.presentationContextProvider = self
            session.prefersEphemeralWebBrowserSession = false
            session.start()
        }
    }

    func isAuthenticated() async -> Bool {
        let url = URL(string: "https://\(appDomain)/api/auth/verify")!

        do {
            let (data, _) = try await URLSession.shared.data(from: url)
            let response = try JSONDecoder().decode(VerifyResponse.self, from: data)
            return response.authenticated
        } catch {
            return false
        }
    }

    func makeRequest(url: URL) async throws -> Data {
        let (data, _) = try await URLSession.shared.data(from: url)
        return data
    }

    // ASWebAuthenticationPresentationContextProviding
    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        return ASPresentationAnchor()
    }
}

struct LoginInfoResponse: Codable {
    let loginUrl: String
    let teamDomain: String

    enum CodingKeys: String, CodingKey {
        case loginUrl = "login_url"
        case teamDomain = "team_domain"
    }
}

struct VerifyResponse: Codable {
    let authenticated: Bool
}

// Usage
let auth = CloudflareAccessAuth(appDomain: "yourdomain.com")
try await auth.initialize()

// Login (opens Safari for authentication)
if !await auth.isAuthenticated() {
    try await auth.login()
}

// Make authenticated requests
let postsUrl = URL(string: "https://yourdomain.com/api/posts")!
let data = try await auth.makeRequest(url: postsUrl)
```

---

## Service Tokens (API/Automation)

For machine-to-machine authentication (no browser required):

### Creating a Service Token

```bash
dais auth create-service-token "CI/CD Pipeline"
```

This will:
1. Guide you through creating a token in Cloudflare dashboard
2. Store the token configuration
3. Provide usage examples

### Using Service Tokens

```bash
curl https://yourdomain.com/api/posts \
  -H "CF-Access-Client-Id: YOUR_CLIENT_ID" \
  -H "CF-Access-Client-Secret: YOUR_CLIENT_SECRET"
```

**Python Example:**
```python
import requests

headers = {
    "CF-Access-Client-Id": "YOUR_CLIENT_ID",
    "CF-Access-Client-Secret": "YOUR_CLIENT_SECRET"
}

response = requests.get(
    "https://yourdomain.com/api/posts",
    headers=headers
)
```

**JavaScript Example:**
```javascript
const response = await fetch('https://yourdomain.com/api/posts', {
  headers: {
    'CF-Access-Client-Id': 'YOUR_CLIENT_ID',
    'CF-Access-Client-Secret': 'YOUR_CLIENT_SECRET'
  }
});
```

---

## CLI Commands

### Setup Authentication

```bash
dais auth setup
```

Interactive setup wizard for Cloudflare Access.

### Create Service Token

```bash
dais auth create-service-token "Token Name" [--duration 8760h]
```

Create an API token for automation.

### List Service Tokens

```bash
dais auth list-service-tokens
```

View configured service tokens.

### Test Authentication

```bash
dais auth test
```

Verify authentication is working correctly.

### Show Status

```bash
dais auth status
```

Display current configuration.

### Documentation Links

```bash
dais auth docs
```

Show helpful Cloudflare Access documentation links.

---

## Security Best Practices

### For Users

1. **Use Strong Identity Providers** - Google, GitHub, Microsoft are recommended
2. **Enable 2FA** - On your identity provider account
3. **Review Access Logs** - Regularly check Cloudflare Access logs
4. **Rotate Service Tokens** - Change tokens periodically
5. **Use Scoped Permissions** - Create separate tokens for different purposes

### For Developers

1. **Use System Browser** - For mobile apps, always use ASWebAuthenticationSession (iOS) or Custom Tabs (Android)
2. **Store Tokens Securely** - Use Keychain (iOS/macOS) or Keystore (Android)
3. **Handle Redirects Properly** - Configure custom URL schemes correctly
4. **Test Token Expiration** - Handle expired authentication gracefully
5. **Use Service Tokens for APIs** - Don't prompt for login in background jobs

---

## Advanced Configuration

### Multiple Identity Providers

You can configure multiple providers (Google + GitHub + Microsoft):

1. Go to Settings → Authentication
2. Click "Add" next to Login methods
3. Configure each provider
4. Users can choose which one to use

### Access Policies

Create granular policies:

```
Policy Name: Admin Access
Action: Allow
Rules:
  - Include: Emails ending in @yourdomain.com
  - Require: Email + Device Posture

Policy Name: Read-Only Access
Action: Allow
Rules:
  - Include: Specific email addresses
  - Require: Email only
```

### Session Duration

Configure how long users stay logged in:

1. Go to your Access application settings
2. Find "Session Duration"
3. Set to desired duration (1 hour to 1 month)

---

## Troubleshooting

### "Access Denied" Error

- **Cause**: Your email is not in the Access policy
- **Fix**: Add your email to the policy in Cloudflare dashboard

### "Invalid AUD Tag" Error

- **Cause**: Application AUD doesn't match configuration
- **Fix**: Run `dais auth setup` again with correct AUD

### Mobile App Not Receiving Callback

- **Cause**: Custom URL scheme not configured
- **Fix**: Add URL scheme to app configuration and Cloudflare Access settings

### Service Token Not Working

- **Cause**: Token not properly configured or expired
- **Fix**: Re-create service token via `dais auth create-service-token`

### Redirect Loop

- **Cause**: Cookies not being set/sent properly
- **Fix**: Ensure credentials: 'include' in fetch options

---

## Migration from Custom Auth

If you previously used custom JWT authentication:

1. Existing JWT tokens will no longer work
2. Users will need to re-authenticate via Cloudflare Access
3. Remove any stored JWT tokens from your apps
4. Update client code to use Cloudflare Access flow
5. Service tokens replace API tokens

---

## Cost

Cloudflare Access pricing (as of 2026):

- **Free**: Up to 50 users
- **Teams**: $7/user/month for unlimited users
- **Enterprise**: Custom pricing

For single-user dais instances, the **free tier is sufficient**.

---

## Resources

- **Cloudflare Access Docs**: https://developers.cloudflare.com/cloudflare-one/
- **Service Tokens**: https://developers.cloudflare.com/cloudflare-one/identity/service-tokens/
- **JWT Validation**: https://developers.cloudflare.com/cloudflare-one/identity/authorization-cookie/validating-json/
- **Mobile Integration**: See examples above

---

**Your dais instance is now secured with enterprise-grade authentication!** 🔐✨
