FROM node:22-bookworm AS builder

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY index.html index.html
COPY tsconfig.json tsconfig.json
COPY src src
RUN npm run build

FROM node:22-bookworm-slim

WORKDIR /app
RUN npm install -g serve@14
COPY --from=builder /app/dist dist

EXPOSE 3000
CMD ["serve", "-s", "dist", "-l", "3000"]
