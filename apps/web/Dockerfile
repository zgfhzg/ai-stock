FROM node:22-bookworm

WORKDIR /app
COPY package.json package.json
RUN npm install
COPY index.html index.html
COPY tsconfig.json tsconfig.json
COPY src src

EXPOSE 3000
CMD ["npm", "run", "dev"]
