use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::config::GitHubAppPublicationCredentials;

const DEFAULT_GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_API_USER_AGENT: &str = "catalyst-continuum-orchestrator";

pub(crate) const GITHUB_APP_API_TRANSPORT: &str = "github_app_api";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ResolvedGitHubPullRequest {
    pub(crate) number: u64,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) is_draft: bool,
    pub(crate) title: String,
    pub(crate) head_ref_name: String,
    pub(crate) base_ref_name: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GitHubRepositoryRef<'a> {
    pub(crate) owner: &'a str,
    pub(crate) name: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DraftGitHubPullRequestRequest<'a> {
    pub(crate) head_branch: &'a str,
    pub(crate) base_branch: &'a str,
    pub(crate) title: &'a str,
    pub(crate) body: &'a str,
}

pub(crate) fn open_or_find_draft_pull_request(
    credentials: &GitHubAppPublicationCredentials,
    repository: GitHubRepositoryRef<'_>,
    pull_request: DraftGitHubPullRequestRequest<'_>,
) -> Result<(String, ResolvedGitHubPullRequest)> {
    open_or_find_draft_pull_request_with_base_url(
        credentials,
        DEFAULT_GITHUB_API_BASE_URL,
        repository,
        pull_request,
    )
}

pub(crate) fn open_or_find_draft_pull_request_with_base_url(
    credentials: &GitHubAppPublicationCredentials,
    api_base_url: &str,
    repository: GitHubRepositoryRef<'_>,
    pull_request: DraftGitHubPullRequestRequest<'_>,
) -> Result<(String, ResolvedGitHubPullRequest)> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(GITHUB_API_USER_AGENT)
        .build()
        .context("failed to build GitHub App HTTP client")?;
    let installation_token =
        request_installation_token(&client, credentials, api_base_url.trim_end_matches('/'))?;

    if let Some(existing) = find_existing_pull_request(
        &client,
        api_base_url.trim_end_matches('/'),
        repository,
        pull_request.head_branch,
        pull_request.base_branch,
        &installation_token,
    )? {
        return Ok(("existing".to_string(), existing));
    }

    let created = create_draft_pull_request(
        &client,
        api_base_url.trim_end_matches('/'),
        repository,
        pull_request,
        &installation_token,
    )?;

    Ok(("created".to_string(), created))
}

fn request_installation_token(
    client: &Client,
    credentials: &GitHubAppPublicationCredentials,
    api_base_url: &str,
) -> Result<String> {
    let app_jwt = build_github_app_jwt(credentials)?;
    let endpoint = format!(
        "{api_base_url}/app/installations/{}/access_tokens",
        credentials.installation_id
    );
    let response: GitHubInstallationTokenResponse = send_github_api_json(
        client
            .post(endpoint)
            .header("Accept", GITHUB_API_ACCEPT)
            .bearer_auth(app_jwt),
        "failed to request GitHub App installation token",
    )?;

    Ok(response.token)
}

fn build_github_app_jwt(credentials: &GitHubAppPublicationCredentials) -> Result<String> {
    let private_key = fs::read(&credentials.private_key_path).with_context(|| {
        format!(
            "failed to read GitHub App private key: {}",
            credentials.private_key_path.display()
        )
    })?;
    let encoding_key = EncodingKey::from_rsa_pem(&private_key)
        .context("failed to parse GitHub App private key PEM")?;
    let issued_at = github_unix_timestamp()?;
    let claims = GitHubAppJwtClaims {
        iat: issued_at.saturating_sub(60),
        exp: issued_at + 540,
        iss: credentials.app_id,
    };

    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &encoding_key)
        .context("failed to encode GitHub App JWT")
}

fn github_unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs())
}

fn find_existing_pull_request(
    client: &Client,
    api_base_url: &str,
    repository: GitHubRepositoryRef<'_>,
    head_branch: &str,
    base_branch: &str,
    installation_token: &str,
) -> Result<Option<ResolvedGitHubPullRequest>> {
    let endpoint = format!(
        "{api_base_url}/repos/{}/{}/pulls",
        repository.owner, repository.name
    );
    let query = GitHubPullRequestListQuery {
        state: "all",
        head: format!("{}:{head_branch}", repository.owner),
        base: base_branch.to_string(),
    };
    let pull_requests: Vec<GitHubApiPullRequest> = send_github_api_json(
        client
            .get(endpoint)
            .header("Accept", GITHUB_API_ACCEPT)
            .bearer_auth(installation_token)
            .query(&query),
        "failed to query existing GitHub pull requests through the GitHub App API",
    )?;

    Ok(pull_requests
        .into_iter()
        .find(|pull_request| pull_request.base.reference == base_branch)
        .map(GitHubApiPullRequest::into_resolved))
}

fn create_draft_pull_request(
    client: &Client,
    api_base_url: &str,
    repository: GitHubRepositoryRef<'_>,
    pull_request: DraftGitHubPullRequestRequest<'_>,
    installation_token: &str,
) -> Result<ResolvedGitHubPullRequest> {
    let endpoint = format!(
        "{api_base_url}/repos/{}/{}/pulls",
        repository.owner, repository.name
    );
    let request = GitHubCreatePullRequestRequest {
        title: pull_request.title,
        head: pull_request.head_branch,
        base: pull_request.base_branch,
        body: pull_request.body,
        draft: true,
    };
    let pull_request: GitHubApiPullRequest = send_github_api_json(
        client
            .post(endpoint)
            .header("Accept", GITHUB_API_ACCEPT)
            .bearer_auth(installation_token)
            .json(&request),
        "failed to create GitHub pull request through the GitHub App API",
    )?;

    Ok(pull_request.into_resolved())
}

fn send_github_api_json<T>(request: RequestBuilder, error_context: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let response = request
        .send()
        .with_context(|| format!("{error_context}: request failed"))?;
    parse_github_api_json(response, error_context)
}

fn parse_github_api_json<T>(response: Response, error_context: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = response
        .text()
        .with_context(|| format!("{error_context}: failed to read response body"))?;
    if !status.is_success() {
        bail!(
            "{error_context}: GitHub API returned {}: {}",
            status.as_u16(),
            body.trim()
        );
    }

    serde_json::from_str(&body)
        .with_context(|| format!("{error_context}: failed to parse response JSON"))
}

#[derive(Debug, Serialize)]
struct GitHubAppJwtClaims {
    iat: u64,
    exp: u64,
    iss: u64,
}

#[derive(Debug, Deserialize)]
struct GitHubInstallationTokenResponse {
    token: String,
}

#[derive(Debug, Serialize)]
struct GitHubPullRequestListQuery {
    state: &'static str,
    head: String,
    base: String,
}

#[derive(Debug, Serialize)]
struct GitHubCreatePullRequestRequest<'a> {
    title: &'a str,
    head: &'a str,
    base: &'a str,
    body: &'a str,
    draft: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubApiPullRequest {
    number: u64,
    #[serde(rename = "html_url")]
    url: String,
    state: String,
    draft: bool,
    title: String,
    head: GitHubApiBranchRef,
    base: GitHubApiBranchRef,
}

impl GitHubApiPullRequest {
    fn into_resolved(self) -> ResolvedGitHubPullRequest {
        ResolvedGitHubPullRequest {
            number: self.number,
            url: self.url,
            state: self.state,
            is_draft: self.draft,
            title: self.title,
            head_ref_name: self.head.reference,
            base_ref_name: self.base.reference,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubApiBranchRef {
    #[serde(rename = "ref")]
    reference: String,
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::SocketAddr,
        os::unix::fs::PermissionsExt,
        path::Path,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use tiny_http::{Header, ListenAddr, Response, Server, StatusCode};

    use super::{
        DraftGitHubPullRequestRequest, GitHubAppPublicationCredentials, GitHubRepositoryRef,
        open_or_find_draft_pull_request_with_base_url,
    };

    const TEST_GITHUB_APP_PRIVATE_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCucVrsytKa4Sab
7PZjKL2Bcwpg6l4nkkoUijAlwBWUMV3jLfv/FxcwzNj7UqS3dGSQ/pP5BBn/DHbO
PwajmhY+gzinm4mRAkIpcKtfYx9TJXied3Xmdm/Go5YMA20W7WMjNSSf99c67NG1
WYpfbM0MmPLHyou1VjofONfdQ8eU1EcQdY25seawSzkId9LAFx9lRawc+wi7WW/S
4nkaXix46vj2fCZO/1mIez7QeRJNzI9BE2gMj4RtMzbfjDtOnNFTMxDGiPqRz7CY
f+UwQKT9th2FiKb8U2JHg7VWh3E/d+TtRnImHpmVwlsvqe7H63ubJ4rw1ob49pHw
wXoKK6VvAgMBAAECggEAAUZDmLPDmXlpovFVEcits0mCsDwBiz0inJBH/d3aTY4P
WX6mp6uktRy1c/rk7OXRaqg/+AjflsveqJ/Cr18Fony22OqjLuXkIrY6o3DE25TM
Q81EG3ya42hNghlv7YDPu2t/1VXW1SUjyiLHYkPCzXeRjV914xTAn0oPQ/fEeGyY
MBoSQqRRq9ku1H2PbNZPodx1ne8K+qahsKu14nXgXMclWL7i/kF7pm5z4/iw3c2h
pYlis9hqqEvg9HljWe0jxXph6ceK/jLnBnsyJwpJMRdvR9/7hyc0EFUy0Y/ADcmn
CNYuGfO5d6C+yHI+t3E/hQla5LT4mmoanjwvxjqhkQKBgQDUV2XKXBs76arof+1L
BB59cOlld/0aHxjcllE3zNhDPSGjsl2M5FMIuybyR4ZVMtIA/mel2x5bmPDj4AMG
EoNEtO0QzdX/JtIO2WYSZG/bNkyHS7/YdnMiIvHeIfhx8kVHqJhf7CpiLwO+uKQ5
R9dryN/Kmes+H5ydhqQN/j4OiQKBgQDSTymJ5yuEUXYE8GmuWSY8LOIrPja0Fr6f
k2aeRIK/NKp8QJvMXL8avtAkcBgMQDcQCrfZ+gC9T/ZlPhX+OdNw/q84bnZUsPw2
qQ/CRhGwXyzlFoJFWTXIKzotaeyQiwtSo2DO0Gnlty+wOKfnXYXJTD/lbVHmkjHP
+myzCTDWNwKBgH8QYYALR9y9QiFo0+Vs7JXh6Dho6dMkwqrVZHqAoPTzctrTFDoI
M0vpOjAG7vKyu4oOspVEHtFvHs8tsIGEuHp3zdidY64QW+i43OSqp2jFAFyBzqZI
kzLdOGDVcSc2c5Ci6bOUzfP88D/Dm7oPLHB6PritDGEbZ4u4ExmwhxAJAoGBAKil
hn/pesIOuP9Y8sY3Ayw6KdvXdfKQUqiQgTflZJuD1jrxbH5C2ZTO8wZlRUN9syoQ
DkKj8jfdiY7CbMyC/oWcFlLAce+URYxnohV+Lu0qRUwn8qs90J0F3Q9R47w9ZAUO
srDl/CWT8o/zvuEP5Br6JDsMoSKulXdcMBKaCimdAoGBAI72ycL8zJSwYGhr/oML
7hDIfNUCz5GdVI2wAbCw9BicdBEaJy+gL42ORJ5hyq/wtSBhYBh6s+o3zpM3FeE2
L0TMcIR+fvKvIDxD6S8sDPdq0ElujeEXdvOobUXnyNsNb733MQ6SdrO92dnwgQ+i
Sya4CmByZsq/XlRdsrkVNpB7
-----END PRIVATE KEY-----
"#;

    #[test]
    fn creates_pull_request_via_github_app_api_when_missing() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-github-app-api-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_root).expect("temp root should be created");
        let credentials = write_test_credentials(&temp_root);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server = Server::http("127.0.0.1:0").expect("test server should bind");
        let server_url = format!("http://{}", listen_addr(&server));
        let requests_for_thread = Arc::clone(&requests);

        let handle = thread::spawn(move || {
            respond_with_json(
                &server,
                &requests_for_thread,
                r#"{"token":"install-token"}"#,
                StatusCode(201),
            );
            respond_with_json(&server, &requests_for_thread, "[]", StatusCode(200));
            respond_with_json(
                &server,
                &requests_for_thread,
                r#"{"number":42,"html_url":"https://github.com/smartit/catalyst-continuum-demo/pull/42","state":"open","draft":true,"title":"PoC: Minimal Container Service","head":{"ref":"continuum/run-00000000"},"base":{"ref":"main"}}"#,
                StatusCode(201),
            );
        });

        let (resolution, pull_request) = open_or_find_draft_pull_request_with_base_url(
            &credentials,
            &server_url,
            GitHubRepositoryRef {
                owner: "smartit",
                name: "catalyst-continuum-demo",
            },
            DraftGitHubPullRequestRequest {
                head_branch: "continuum/run-00000000",
                base_branch: "main",
                title: "PoC: Minimal Container Service",
                body: "Draft PR body",
            },
        )
        .expect("GitHub App API PR creation should succeed");

        handle.join().expect("test server thread should join");

        assert_eq!(resolution, "created");
        assert_eq!(pull_request.number, 42);
        assert_eq!(
            pull_request.url,
            "https://github.com/smartit/catalyst-continuum-demo/pull/42"
        );
        assert!(pull_request.is_draft);
        assert_eq!(pull_request.head_ref_name, "continuum/run-00000000");
        assert_eq!(pull_request.base_ref_name, "main");

        let recorded = requests.lock().expect("recorded requests should lock");
        assert_eq!(recorded.len(), 3);
        assert!(recorded[0].method == "POST");
        assert_eq!(recorded[0].url, "/app/installations/456/access_tokens");
        assert!(
            recorded[0]
                .authorization
                .as_deref()
                .is_some_and(|value| value.starts_with("Bearer "))
        );
        assert_eq!(
            recorded[1].url,
            "/repos/smartit/catalyst-continuum-demo/pulls?state=all&head=smartit%3Acontinuum%2Frun-00000000&base=main"
        );
        assert_eq!(
            recorded[2].body,
            r#"{"title":"PoC: Minimal Container Service","head":"continuum/run-00000000","base":"main","body":"Draft PR body","draft":true}"#
        );

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn reuses_existing_pull_request_via_github_app_api() {
        let temp_root = std::env::temp_dir().join(format!(
            "continuum-github-app-api-existing-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_root).expect("temp root should be created");
        let credentials = write_test_credentials(&temp_root);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server = Server::http("127.0.0.1:0").expect("test server should bind");
        let server_url = format!("http://{}", listen_addr(&server));
        let requests_for_thread = Arc::clone(&requests);

        let handle = thread::spawn(move || {
            respond_with_json(
                &server,
                &requests_for_thread,
                r#"{"token":"install-token"}"#,
                StatusCode(201),
            );
            respond_with_json(
                &server,
                &requests_for_thread,
                r#"[{"number":43,"html_url":"https://github.com/smartit/catalyst-continuum-demo/pull/43","state":"open","draft":true,"title":"PoC: Minimal Container Service","head":{"ref":"continuum/run-00000000"},"base":{"ref":"main"}}]"#,
                StatusCode(200),
            );
        });

        let (resolution, pull_request) = open_or_find_draft_pull_request_with_base_url(
            &credentials,
            &server_url,
            GitHubRepositoryRef {
                owner: "smartit",
                name: "catalyst-continuum-demo",
            },
            DraftGitHubPullRequestRequest {
                head_branch: "continuum/run-00000000",
                base_branch: "main",
                title: "PoC: Minimal Container Service",
                body: "Draft PR body",
            },
        )
        .expect("existing GitHub App API PR should be reused");

        handle.join().expect("test server thread should join");

        assert_eq!(resolution, "existing");
        assert_eq!(pull_request.number, 43);

        let recorded = requests.lock().expect("recorded requests should lock");
        assert_eq!(recorded.len(), 2);
        assert!(recorded.iter().all(|request| {
            request
                .authorization
                .as_deref()
                .is_some_and(|value| value.starts_with("Bearer "))
        }));

        let _ = fs::remove_dir_all(temp_root);
    }

    fn write_test_credentials(temp_root: &Path) -> GitHubAppPublicationCredentials {
        let private_key_path = temp_root.join("github-app.pem");
        fs::write(&private_key_path, TEST_GITHUB_APP_PRIVATE_KEY)
            .expect("test private key should be written");
        let mut permissions = fs::metadata(&private_key_path)
            .expect("test private key metadata should exist")
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&private_key_path, permissions)
            .expect("test private key permissions should be set");

        GitHubAppPublicationCredentials {
            app_id: 123,
            installation_id: 456,
            private_key_path,
        }
    }

    fn listen_addr(server: &Server) -> SocketAddr {
        match server.server_addr() {
            ListenAddr::IP(addr) => addr,
            other => panic!("unexpected test server address: {other:?}"),
        }
    }

    fn respond_with_json(
        server: &Server,
        requests: &Arc<Mutex<Vec<RecordedRequest>>>,
        body: &str,
        status_code: StatusCode,
    ) {
        let mut request = server
            .recv_timeout(Duration::from_secs(5))
            .expect("test server should receive request")
            .expect("test server should not time out");
        let snapshot = record_request(&mut request);
        requests
            .lock()
            .expect("recorded requests should lock")
            .push(snapshot);
        let response = Response::from_string(body)
            .with_status_code(status_code)
            .with_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("content type header should build"),
            );
        request
            .respond(response)
            .expect("test server response should succeed");
    }

    fn record_request(request: &mut tiny_http::Request) -> RecordedRequest {
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("request body should be readable");

        RecordedRequest {
            method: request.method().as_str().to_string(),
            url: request.url().to_string(),
            authorization: request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string()),
            body,
        }
    }

    #[derive(Debug)]
    struct RecordedRequest {
        method: String,
        url: String,
        authorization: Option<String>,
        body: String,
    }
}
