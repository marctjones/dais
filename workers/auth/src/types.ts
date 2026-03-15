/**
 * Type definitions for Cloudflare Access Auth Worker
 */

export interface Env {
  // D1 Database (for optional token storage)
  DB?: D1Database;

  // Cloudflare Access Configuration
  CLOUDFLARE_ACCESS_TEAM_DOMAIN: string;  // e.g., "myteam.cloudflareaccess.com"
  CLOUDFLARE_ACCESS_AUD: string;           // Application AUD tag from Access dashboard

  // Optional: Restrict to specific email
  ALLOWED_EMAIL?: string;                  // e.g., "you@example.com"

  // Optional: Application domain
  APP_DOMAIN?: string;                     // e.g., "yourdomain.com"

  // Service Tokens (JSON array)
  SERVICE_TOKENS?: string;                 // e.g., '[{"clientId":"xxx","clientSecret":"yyy","name":"API"}]'
}
