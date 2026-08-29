//! omegon-codescan — code and knowledge indexing for codebase_search.
pub mod bm25;
pub mod cache;
pub mod code;
pub mod indexer;
pub mod knowledge;

pub use bm25::BM25Index;
pub use cache::ScanCache;
pub use code::{
    CodeChunk, CodeScanner, ExtractionConfidence, ExtractionStrategy, is_supported_code_extension,
};
pub use indexer::Indexer;
pub use knowledge::{KnowledgeChunk, KnowledgeDirs, KnowledgeScanner};
pub use omegon_codescan_contracts::{ChunkType, IndexStats, SearchChunk, SearchScope};
