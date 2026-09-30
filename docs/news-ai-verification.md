# News AI Verification

뉴스 기반 AI 분석의 실제 동작, 처리 시간, 토큰 사용량, 비용 추정을 기록한다.

## Current Settings

- Provider: OpenAI API
- Model: `gpt-4o-mini`
- Batch size: 10 articles
- Analysis enabled: true
- API key: configured in local `.env`, not committed

## 2026-09-30 Verification

### Environment

- Strategy service: local FastAPI server on `127.0.0.1:8090`
- API service: local Rust server on `127.0.0.1:8080`
- Docker was unavailable in the current shell, so services were run directly.

### Collection Result

- Collected articles: 103
- Analyzed articles after verification runs: 40
- Pending analysis: 73
- Failed analysis: 0
- News database size: about 0.15 MB

### Analysis Batch Result

- Requested: 10
- Analyzed: 10
- Failed: 0
- Model: `gpt-4o-mini`
- Elapsed time: 9.343 seconds
- Input tokens: 1,268
- Output tokens: 869
- Total tokens: 2,137
- Estimated cost: $0.0007116

Cost estimate uses `gpt-4o-mini` pricing of $0.15 per 1M input tokens and $0.60 per 1M output tokens, based on OpenAI API pricing at the time of verification.

### Initial Quality Notes

- Market-index articles without a named company correctly produced no related stock in most cases.
- Semiconductor-market articles sometimes mapped broad sector news to Samsung Electronics and SK hynix.
- This confirms the need for the next roadmap item: exact matching between AI company names and `data/stocks.json`, with uncertain results excluded from trading candidates.

## Implementation Notes

- Strategy responses now include token usage when the OpenAI SDK returns it.
- API responses for `POST /api/news/analyze` now include elapsed time, token counts, and estimated cost.
- The web dashboard shows the latest AI analysis batch summary after a manual analysis run.

## Security Note

The OpenAI API key used for this verification was provided during chat. Rotate it after the verification session and replace the local `.env` value with a fresh key.
