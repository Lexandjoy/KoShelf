# 1) Build dos assets (Tailwind)
FROM node:18-alpine AS build

WORKDIR /app

# Copiar package.json e instalar todas as dependências (incluindo dev)
COPY package*.json ./
RUN npm ci

# Copiar o código e gerar CSS otimizado
COPY . .
RUN npm run build-css-prod

# 2) Servir estáticos com Nginx
FROM nginx:alpine

# Remover conteúdo default do Nginx
RUN rm -rf /usr/share/nginx/html/*

# Copiar HTML/JS (assumindo index.html na raiz) e assets gerados
COPY index.html /usr/share/nginx/html/
COPY assets /usr/share/nginx/html/assets

# Expor porta 3000 para o Coolify
EXPOSE 3000

# Iniciar Nginx em primeiro plano
CMD ["nginx", "-g", "daemon off;"]
