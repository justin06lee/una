"""QLoRA fine-tuning of the cleanup LLM on (raw -> polished) style pairs.

Heavy modules are imported inside train_style_adapter so this stays importable in
the serving venv. On CUDA the base model loads 4-bit (QLoRA) so a 3B model trains
inside 6 GB; on CPU (tests/dev) it loads fp32 without quantization.
"""

from __future__ import annotations

import gc
import logging
from collections.abc import Callable, Sequence
from pathlib import Path

from ..config import CleanupConfig
from .style_dataset import StyleSample, messages_for

log = logging.getLogger(__name__)

TARGET_MODULES = ["q_proj", "k_proj", "v_proj", "o_proj"]


def train_style_adapter(
    samples: Sequence[StyleSample],
    run_dir: Path,
    hp: dict,
    cleanup_cfg: CleanupConfig,
    *,
    device: str = "cuda",
    on_progress: Callable[[float], None] | None = None,
) -> Path:
    """Train a LoRA adapter on chat-formatted style pairs; save to run_dir/adapter.

    `hp` carries the run's hyperparams_json: style_base_hf_model, style_lora_r,
    style_lora_alpha, style_lora_dropout, style_learning_rate, style_epochs,
    style_batch_size, style_grad_accum, style_max_seq_len.
    """
    import torch
    from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
    from torch.utils.data import Dataset
    from transformers import (
        AutoModelForCausalLM,
        AutoTokenizer,
        Trainer,
        TrainerCallback,
        TrainingArguments,
    )

    use_cuda = device == "cuda" and torch.cuda.is_available()
    base = hp["style_base_hf_model"]
    max_len = int(hp["style_max_seq_len"])
    log.info("loading %s for style LoRA training on %d pairs", base, len(samples))

    tokenizer = AutoTokenizer.from_pretrained(base)
    if tokenizer.pad_token_id is None:
        tokenizer.pad_token = tokenizer.eos_token

    if use_cuda:
        from transformers import BitsAndBytesConfig

        quant = BitsAndBytesConfig(
            load_in_4bit=True,
            bnb_4bit_quant_type="nf4",
            bnb_4bit_compute_dtype=torch.float16,
            bnb_4bit_use_double_quant=True,
        )
        model = AutoModelForCausalLM.from_pretrained(
            base, quantization_config=quant, device_map={"": 0}
        )
        model = prepare_model_for_kbit_training(
            model, gradient_checkpointing_kwargs={"use_reentrant": False}
        )
    else:
        model = AutoModelForCausalLM.from_pretrained(base, torch_dtype=torch.float32)
        model.gradient_checkpointing_enable(
            gradient_checkpointing_kwargs={"use_reentrant": False}
        )
        model.enable_input_require_grads()
    model.config.use_cache = False

    lora = LoraConfig(
        r=int(hp["style_lora_r"]),
        lora_alpha=int(hp["style_lora_alpha"]),
        lora_dropout=float(hp["style_lora_dropout"]),
        target_modules=TARGET_MODULES,
        bias="none",
        task_type="CAUSAL_LM",
    )
    model = get_peft_model(model, lora)
    model.print_trainable_parameters()

    def encode(sample: StyleSample) -> dict:
        prompt_ids = tokenizer.apply_chat_template(
            messages_for(cleanup_cfg, sample.raw_text, sample.app_name),
            add_generation_prompt=True,
        )
        target_ids = tokenizer(
            sample.polished_text, add_special_tokens=False
        ).input_ids + [tokenizer.eos_token_id]
        input_ids = (prompt_ids + target_ids)[:max_len]
        labels = ([-100] * len(prompt_ids) + target_ids)[:max_len]
        return {"input_ids": input_ids, "labels": labels}

    class StyleDataset(Dataset):
        def __len__(self) -> int:
            return len(samples)

        def __getitem__(self, index: int) -> dict:
            return encode(samples[index])

    def collate(batch: list[dict]) -> dict:
        width = max(len(item["input_ids"]) for item in batch)
        pad = tokenizer.pad_token_id
        input_ids, labels, attention = [], [], []
        for item in batch:
            n = width - len(item["input_ids"])
            input_ids.append(item["input_ids"] + [pad] * n)
            labels.append(item["labels"] + [-100] * n)
            attention.append([1] * len(item["input_ids"]) + [0] * n)
        return {
            "input_ids": torch.tensor(input_ids),
            "labels": torch.tensor(labels),
            "attention_mask": torch.tensor(attention),
        }

    class ProgressCallback(TrainerCallback):
        def on_step_end(self, args, state, control, **kwargs):
            if on_progress is not None and state.max_steps:
                on_progress(state.global_step / state.max_steps)

    training_args = TrainingArguments(
        output_dir=str(run_dir),
        per_device_train_batch_size=int(hp["style_batch_size"]),
        gradient_accumulation_steps=int(hp["style_grad_accum"]),
        learning_rate=float(hp["style_learning_rate"]),
        num_train_epochs=float(hp["style_epochs"]),
        warmup_steps=20,
        lr_scheduler_type="cosine",
        fp16=use_cuda,
        eval_strategy="no",
        save_strategy="no",
        logging_steps=5,
        report_to=[],
        remove_unused_columns=False,
        label_names=["labels"],
        gradient_checkpointing=use_cuda,
        gradient_checkpointing_kwargs={"use_reentrant": False},
    )
    trainer = Trainer(
        model=model,
        args=training_args,
        train_dataset=StyleDataset(),
        data_collator=collate,
        callbacks=[ProgressCallback()],
    )
    trainer.train()

    adapter_dir = run_dir / "adapter"
    model.save_pretrained(str(adapter_dir))
    log.info("saved style LoRA adapter to %s", adapter_dir)

    del trainer, model
    gc.collect()
    import torch as _torch

    if use_cuda:
        _torch.cuda.empty_cache()
    return adapter_dir
