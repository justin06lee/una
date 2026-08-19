from fastapi import APIRouter

from . import dictations, dictionary, model_registry, settings_api, system, training

router = APIRouter(prefix="/v1")
router.include_router(dictations.router)
router.include_router(dictionary.router)
router.include_router(training.router)
router.include_router(model_registry.router)
router.include_router(settings_api.router)
router.include_router(system.router)
