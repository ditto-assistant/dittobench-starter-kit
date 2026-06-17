# ---- build stage ----
FROM golang:1.23-alpine AS build
WORKDIR /src

# Cache deps first.
COPY go.mod ./
RUN go mod download || true

COPY . .
# Static build for a minimal runtime image.
RUN CGO_ENABLED=0 GOOS=linux go build -trimpath -ldflags="-s -w" \
    -o /out/dittobench-miner ./cmd/dittobench-miner

# ---- runtime stage ----
FROM gcr.io/distroless/static-debian12:nonroot
WORKDIR /app
COPY --from=build /out/dittobench-miner /usr/local/bin/dittobench-miner

# The validator POSTs cases to /run on this port.
EXPOSE 8080
ENV PORT=8080

# OPENROUTER_API_KEY must be provided at runtime (-e OPENROUTER_API_KEY=...).
ENTRYPOINT ["dittobench-miner", "serve"]
CMD ["-port", "8080"]
