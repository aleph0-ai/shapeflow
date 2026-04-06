use base64::{Engine as _, engine::general_purpose::STANDARD};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, CallToolResult, Content, RawAudioContent, RawContent, ResourceContents,
        ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
};

use crate::{flow, flow::Difficulty, stimulus};

#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct GetEvalSampleArgs {
    /// Session seed used to deterministically regenerate the sample.
    pub seed: u64,
    /// Difficulty: easy, medium, or hard.
    pub difficulty: String,
    /// Modality: image, video, text, tabular, or sound.
    pub modality: String,
    /// Scene index for the requested sample.
    pub idx: u32,
}

#[derive(Clone)]
pub struct HumanEvalMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl HumanEvalMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Return a deterministic native sample for (seed,difficulty,modality,idx)")]
    fn get_eval_sample(
        &self,
        Parameters(args): Parameters<GetEvalSampleArgs>,
    ) -> Result<CallToolResult, McpError> {
        let difficulty = Difficulty::from_str(&args.difficulty)
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;

        let modality_key = args.modality.to_ascii_lowercase();
        let modality = flow::Modality::from_str(&modality_key).ok_or_else(|| {
            McpError::invalid_params(
                "invalid modality; expected image|video|text|tabular|sound",
                None,
            )
        })?;

        let payload = stimulus::build_ai_native_sample(args.seed, difficulty, modality, args.idx)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;

        let sample_uri = format!(
            "shapeflow://sample/{seed}/{difficulty}/{modality}/{idx}",
            seed = args.seed,
            difficulty = difficulty.as_str(),
            modality = modality.as_str(),
            idx = args.idx
        );

        let result = match payload {
            stimulus::NativeSamplePayload::Text { text, .. } => {
                CallToolResult::success(vec![Content::text(text)])
            }
            stimulus::NativeSamplePayload::Binary { mime_type, bytes } => {
                let blob = STANDARD.encode(&bytes);
                let content = if mime_type == "image/gif" {
                    vec![Content::resource(
                        ResourceContents::blob(blob, sample_uri).with_mime_type(mime_type),
                    )]
                } else if mime_type.starts_with("image/") {
                    vec![Content::image(blob, mime_type)]
                } else if mime_type.starts_with("audio/") {
                    vec![
                        RawContent::Audio(RawAudioContent {
                            data: blob,
                            mime_type,
                        })
                        .no_annotation(),
                    ]
                } else {
                    vec![Content::resource(
                        ResourceContents::blob(blob, sample_uri).with_mime_type(mime_type),
                    )]
                };
                CallToolResult::success(content)
            }
        };

        Ok(result)
    }
}

#[tool_handler]
impl ServerHandler for HumanEvalMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Call get_eval_sample(seed,difficulty,modality,idx) to fetch deterministic native test samples."
                    .to_string(),
            )
    }
}
