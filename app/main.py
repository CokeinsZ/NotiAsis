from fastapi import FastAPI

from app.api.webhook import router as webhook_router


def create_app() -> FastAPI:
    app = FastAPI(title="NotiAsis")
    app.include_router(webhook_router)
    return app


app = create_app()
