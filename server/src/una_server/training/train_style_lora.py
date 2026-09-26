"""QLoRA fine-tuning of the cleanup LLM: supervised pairs, then an optional DPO pass.

Heavy modules are imported inside train_style_adapter so this stays importable in
the serving venv. On CUDA the base model loads 4-bit (QLoRA) so a 3B model trains
inside 6 GB; on CPU (tests/dev) it loads fp32 without quantization.

Stage 1 (SFT) teaches the model to produce the target text for each transcript.
Stage 2 (DPO) sharpens that with preference pairs — the user's own version of a
dictation against what the model produced — by raising the likelihood of the
preferred output relative to the dispreferred one, measured against the model as it
stood after stage 1. The reference log-probabilities are computed once up front, so no
second copy of the model is needed in memory.
"""

from __future__ import annotations

import gc
import logging
import math
import random
from collections.abc import Callable, Sequence
from pathlib import Path

from ..config import CleanupConfig
from .style_dataset import PreferencePair, StyleSample, messages_for

log = logging.getLogger(__name__)

TARGET_MODULES = ["q_proj", "k_proj", "v_proj", "o_proj"]
MAX_DPO_REPLY_TOKENS = 256


def chat_ids(tokenizer, messages: list[dict]) -> list[int]:
    """Token ids of a chat prompt ending where the assistant's reply begins.

    Newer transformers return a BatchEncoding from apply_chat_template, older ones a
    plain list; accept either.
    """
    ids = tokenizer.apply_chat_template(messages, add_generation_prompt=True, tokenize=True)
    if hasattr(ids, "keys"):
        ids = ids["input_ids"]
    return list(ids)


def train_style_adapter(
    samples: Sequence[StyleSample],
    run_dir: Path,
    hp: dict,
    cleanup_cfg: CleanupConfig,
    *,
    device: str = "cuda",
    on_progress: Callable[[float], None] | None = None,
    preferences: Sequence[PreferencePair] = (),
) -> tuple[Path, dict]:
    """Train a LoRA adapter; save it to run_dir/adapter. Returns (adapter dir, DPO stats).

    `hp` carries the run's hyperparams_json (the style_* training settings).
    DPO runs after SFT when `preferences` is non-empty.
    """
    import torch
    from peft import LoraConfig, get_peft_model
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
    sft_share = 0.8 if preferences else 1.0
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
        # fp16, not the checkpoint's bf16: Turing cards have no fast bf16.
        model = AutoModelForCausalLM.from_pretrained(
            base, quantization_config=quant, device_map={"": 0}, dtype=torch.float16
        )
        # prepare_model_for_kbit_training would also upcast every unquantized weight to
        # fp32 — on Llama 3.2 that includes the 128k-token embedding tied to the output
        # head, ~1.6 GB a 6 GB card cannot spare. Everything it does besides that:
        for param in model.parameters():
            param.requires_grad = False
        model.gradient_checkpointing_enable(
            gradient_checkpointing_kwargs={"use_reentrant": False}
        )
        model.enable_input_require_grads()
    else:
        model = AutoModelForCausalLM.from_pretrained(base, dtype=torch.float32)
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
        prompt_ids = chat_ids(tokenizer, messages_for(cleanup_cfg, sample.raw_text, sample.app_name))
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
                on_progress(sft_share * state.global_step / state.max_steps)

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
    del trainer
    gc.collect()
    if use_cuda:
        torch.cuda.empty_cache()

    # Saved before DPO, so stage 2 failing — it is the one that pushes a 6 GB card
    # hardest — costs the refinement, not the hour of supervised training.
    adapter_dir = run_dir / "adapter"
    model.save_pretrained(str(adapter_dir))
    log.info("saved supervised style adapter to %s", adapter_dir)

    dpo_stats: dict = {}
    if preferences:
        def dpo_progress(fraction: float) -> None:
            if on_progress is not None:
                on_progress(sft_share + (1 - sft_share) * fraction)

        try:
            dpo_stats = dpo(model, tokenizer, preferences, hp, cleanup_cfg, use_cuda=use_cuda,
                            on_progress=dpo_progress)
        except Exception as exc:  # OOM above all; keep the supervised adapter
            log.exception("dpo failed; keeping the supervised adapter")
            dpo_stats = {"error": f"{type(exc).__name__}: {str(exc)[:160]}"}
        else:
            model.save_pretrained(str(adapter_dir))
            log.info("saved preference-tuned style adapter to %s", adapter_dir)

    del model
    gc.collect()
    if use_cuda:
        torch.cuda.empty_cache()
    return adapter_dir, dpo_stats


def dpo(
    model,
    tokenizer,
    pairs: Sequence[PreferencePair],
    hp: dict,
    cleanup_cfg: CleanupConfig,
    *,
    use_cuda: bool,
    on_progress: Callable[[float], None] | None = None,
) -> dict:
    """Direct preference optimization of the LoRA adapter, in place.

    Loss per pair: -log σ(β · [(log π(chosen) − log π_ref(chosen)) −
    (log π(rejected) − log π_ref(rejected))]), with π_ref the model as it stands on
    entry (after SFT). Returns summary stats for the run's notes.
    """
    import torch
    import torch.nn.functional as F

    beta = float(hp["style_dpo_beta"])
    max_len = int(hp["style_max_seq_len"])
    accum = int(hp["style_grad_accum"])
    device = next(model.parameters()).device

    def encode(pair: PreferencePair, response: str):
        prompt = chat_ids(tokenizer, messages_for(cleanup_cfg, pair.raw_text, pair.app_name))
        # A rambling rejected output (a reply rather than a cleanup) needs no more than
        # its start to be told apart; capping it keeps the step's memory bounded.
        reply = tokenizer(response, add_special_tokens=False).input_ids[:MAX_DPO_REPLY_TOKENS]
        ids = (prompt + reply + [tokenizer.eos_token_id])[:max_len]
        return torch.tensor(ids, device=device), len(prompt)

    encoded = []
    for pair in pairs:
        chosen, rejected = encode(pair, pair.chosen), encode(pair, pair.rejected)
        # a prompt that fills the window leaves no response tokens to compare
        if chosen[1] < len(chosen[0]) and rejected[1] < len(rejected[0]):
            encoded.append((chosen, rejected))
    if not encoded:
        log.info("dpo: no usable pairs")
        return {"pairs": 0}

    def logprob(ids, start: int):
        # Only the response's positions need vocabulary logits: with a 128k vocabulary,
        # logits for the whole prompt are most of the memory a step would use.
        keep = len(ids) - start + 1
        with torch.autocast("cuda", dtype=torch.float16, enabled=use_cuda):
            logits = model(input_ids=ids.unsqueeze(0), logits_to_keep=keep).logits[0, :-1]
        logp = torch.log_softmax(logits.float(), dim=-1)
        return logp.gather(-1, ids[start:].unsqueeze(-1)).sum()

    model.eval()
    with torch.no_grad():
        reference = [(logprob(*c).item(), logprob(*r).item()) for c, r in encoded]
    model.train()

    params = [p for p in model.parameters() if p.requires_grad]
    optimizer = torch.optim.AdamW(params, lr=float(hp["style_dpo_learning_rate"]))
    scaler = torch.amp.GradScaler("cuda", enabled=use_cuda)
    steps = max(1, math.ceil(len(encoded) * float(hp["style_dpo_epochs"])))
    rng = random.Random(0)
    order: list[int] = []
    while len(order) < steps:
        epoch = list(range(len(encoded)))
        rng.shuffle(epoch)
        order += epoch
    order = order[:steps]

    losses, wins = [], 0
    for step, index in enumerate(order):
        (chosen, rejected), (ref_chosen, ref_rejected) = encoded[index], reference[index]
        # Holding both sequences' graphs at once is what a 6 GB card can't do, so the
        # loss is backpropagated one sequence at a time. With m the margin below,
        # d loss / d log π(chosen) = -β σ(-m) and d loss / d log π(rejected) = +β σ(-m):
        # a no-grad pass finds m, then each sequence gets its own weighted backward.
        with torch.no_grad():
            margin = beta * (
                (logprob(*chosen) - ref_chosen) - (logprob(*rejected) - ref_rejected)
            ).item()
        weight = beta * float(torch.sigmoid(torch.tensor(-margin)))
        scaler.scale(-weight * logprob(*chosen) / accum).backward()
        scaler.scale(weight * logprob(*rejected) / accum).backward()
        losses.append(float(-F.logsigmoid(torch.tensor(margin))))
        wins += int(margin > 0)
        if (step + 1) % accum == 0 or step + 1 == steps:
            scaler.step(optimizer)
            scaler.update()
            optimizer.zero_grad(set_to_none=True)
        if step % 20 == 0:
            recent = losses[-20:]
            log.info("dpo step %d/%d: loss %.4f", step + 1, steps, sum(recent) / len(recent))
        if on_progress is not None:
            on_progress((step + 1) / steps)

    tail = losses[-min(len(losses), 50):]
    stats = {
        "pairs": len(encoded),
        "steps": steps,
        "final_loss": round(sum(tail) / len(tail), 4),
        "preferred_rate": round(wins / steps, 3),
    }
    log.info("dpo done: %s", stats)
    return stats
