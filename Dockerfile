FROM node:18-alpine

WORKDIR /app

# Copiar package.json e package-lock.json (se existir)
COPY package*.json ./

# Instalar dependências
RUN npm ci --only=production

# Copiar o código da aplicação
COPY . .

# Build da aplicação (se necessário)
RUN npm run build

# Expor a porta
EXPOSE 3000

# Comando para iniciar
CMD ["npm", "start"]
