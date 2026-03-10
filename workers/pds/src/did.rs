use worker::*;
use serde_json::json;

/// Handle DID document request
///
/// Returns a did:web document for social.dais.social
/// This allows AT Protocol to resolve the identity
pub async fn handle_did_document(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let domain = ctx.env.var("DOMAIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());

    let pds_hostname = ctx.env.var("PDS_HOSTNAME")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| format!("pds.dais.social"));

    // Construct did:web identifier
    let did = format!("did:web:{}", domain);

    // DID document following AT Protocol spec
    let did_doc = json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/suites/secp256k1-2019/v1"
        ],
        "id": did,
        "alsoKnownAs": [
            format!("at://{}", domain)
        ],
        "verificationMethod": [
            {
                "id": format!("{}#atproto", did),
                "type": "Multikey",
                "controller": did,
                "publicKeyMultibase": "zQ3shXjHeiBuRCKmM36cuYnm7YEMzhGnCmCyW92sRJ9pribSF"
            }
        ],
        "service": [
            {
                "id": "#atproto_pds",
                "type": "AtprotoPersonalDataServer",
                "serviceEndpoint": format!("https://{}", pds_hostname)
            }
        ]
    });

    Response::from_json(&did_doc)
}
