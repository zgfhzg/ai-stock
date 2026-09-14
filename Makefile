COMPOSE ?= docker compose

.PHONY: build up down restart logs ps health pull

build:
	$(COMPOSE) build

up:
	$(COMPOSE) up -d --build

down:
	$(COMPOSE) down

restart:
	$(COMPOSE) restart

logs:
	$(COMPOSE) logs -f --tail=120

ps:
	$(COMPOSE) ps

health:
	$(COMPOSE) ps
	@printf '\nAPI: '
	@curl -fsS http://127.0.0.1:8080/health
	@printf '\nStrategy: '
	@curl -fsS http://127.0.0.1:8090/health
	@printf '\nWeb: '
	@curl -fsS http://127.0.0.1:3000 >/dev/null && printf 'ok\n'

pull:
	git pull --ff-only
