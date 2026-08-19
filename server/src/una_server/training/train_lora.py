"""LoRA fine-tuning of Whisper on correction pairs.

Everything heavy (torch/transformers/peft/soundfile) is imported inside train_adapter so the
module stays importable in the serving venv.
"""

from __future__ import annotations

import gc
import logging
from collections.abc import Callable, Sequence
from pathlib import Path

from .dataset import Sample

log = logging.getLogger(__name__)

TARGET_MODULES = ["q_proj", "v_proj"]


def train_adapter(
    samples: Sequence[Sample],
    run_dir: Path,
    hp: dict,
    *,
    device: str = "cuda",
    language: str | None = "en",
    on_progress: Callable[[float], None] | None = None,
) -> Path:
    """Train a LoRA adapter on `samples` and save it to run_dir/adapter.

    `hp` carries the run's hyperparams_json: base_hf_model, lora_r, lora_alpha, lora_dropout,
    learning_rate, epochs, batch_size, grad_accum. `on_progress` receives the training
    fraction in [0, 1]; the caller maps it onto the run row's overall progress.
    """
    import soundfile as sf
    import torch
    from peft import LoraConfig, get_peft_model
    from torch.utils.data import Dataset
    from transformers import (
        Seq2SeqTrainer,
        Seq2SeqTrainingArguments,
        TrainerCallback,
        WhisperForConditionalGeneration,
        WhisperProcessor,
    )

    use_cuda = device == "cuda" and torch.cuda.is_available()
    dtype = torch.float16 if use_cuda else torch.float32
    base_hf_model = hp["base_hf_model"]
    log.info("loading %s (%s) for LoRA training on %d samples", base_hf_model, dtype, len(samples))

    processor = WhisperProcessor.from_pretrained(base_hf_model)
    if language:
        processor.tokenizer.set_prefix_tokens(language=language, task="transcribe")

    model = WhisperForConditionalGeneration.from_pretrained(base_hf_model, torch_dtype=dtype)
    decoder_start_token_id = model.config.decoder_start_token_id
    model.config.forced_decoder_ids = None
    model.config.use_cache = False  # incompatible with gradient checkpointing
    model.gradient_checkpointing_enable(gradient_checkpointing_kwargs={"use_reentrant": False})
    model.enable_input_require_grads()  # keeps checkpointed activations connected to LoRA grads

    lora = LoraConfig(
        r=int(hp["lora_r"]),
        lora_alpha=int(hp["lora_alpha"]),
        lora_dropout=float(hp["lora_dropout"]),
        target_modules=TARGET_MODULES,
        bias="none",
    )
    model = get_peft_model(model, lora)
    model.print_trainable_parameters()
    for param in model.parameters():
        if param.requires_grad:
            param.data = param.data.float()  # fp32 adapters: AMP cannot unscale fp16 grads

    class WhisperDataset(Dataset):
        def __len__(self) -> int:
            return len(samples)

        def __getitem__(self, index: int) -> dict:
            sample = samples[index]
            audio, sample_rate = sf.read(sample.audio_path, dtype="float32", always_2d=False)
            if audio.ndim > 1:
                audio = audio.mean(axis=1)  # stored audio is mono; belt and suspenders
            features = processor.feature_extractor(audio, sampling_rate=sample_rate)
            return {
                "input_features": features.input_features[0],
                "labels": processor.tokenizer(sample.text).input_ids,
            }

    def collate(batch: list[dict]) -> dict:
        features = processor.feature_extractor.pad(
            [{"input_features": item["input_features"]} for item in batch], return_tensors="pt"
        )
        labels_batch = processor.tokenizer.pad(
            [{"input_ids": item["labels"]} for item in batch], return_tensors="pt"
        )
        labels = labels_batch["input_ids"].masked_fill(labels_batch["attention_mask"].ne(1), -100)
        if (labels[:, 0] == decoder_start_token_id).all().cpu().item():
            labels = labels[:, 1:]  # the model shifts right and re-adds BOS itself
        features["labels"] = labels
        return features

    class ProgressCallback(TrainerCallback):
        def on_step_end(self, args, state, control, **kwargs):
            if on_progress is not None and state.max_steps:
                on_progress(state.global_step / state.max_steps)

    training_args = Seq2SeqTrainingArguments(
        output_dir=str(run_dir),
        per_device_train_batch_size=int(hp["batch_size"]),
        gradient_accumulation_steps=int(hp["grad_accum"]),
        learning_rate=float(hp["learning_rate"]),
        num_train_epochs=float(hp["epochs"]),
        warmup_steps=50,
        lr_scheduler_type="cosine",
        fp16=use_cuda,
        eval_strategy="no",
        save_strategy="no",
        logging_steps=5,
        report_to=[],
        remove_unused_columns=False,
        label_names=["labels"],
    )
    trainer = Seq2SeqTrainer(
        model=model,
        args=training_args,
        train_dataset=WhisperDataset(),
        data_collator=collate,
        callbacks=[ProgressCallback()],
    )
    trainer.train()

    adapter_dir = run_dir / "adapter"
    model.save_pretrained(str(adapter_dir))
    log.info("saved LoRA adapter to %s", adapter_dir)

    del trainer, model
    gc.collect()
    if use_cuda:
        torch.cuda.empty_cache()
    return adapter_dir
