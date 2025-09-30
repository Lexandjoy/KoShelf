FROM node:18-alpine

WORKDIR /app

# Copiar package.json e package-lock.json (se existir)
COPY package*.json ./

# Instalar dependências
RUN npm ci --omit=dev

# Copiar todo o código
COPY . .

# Expor a porta usada pela aplicação
EXPOSE 3000

# Comando para iniciar a aplicação diretamente
CMD ["npm", "start"]
