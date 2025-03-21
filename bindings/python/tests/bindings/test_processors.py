import json
import pickle

import pytest

from tokenizers import Tokenizer
from tokenizers.models import BPE
from tokenizers.pre_tokenizers import ByteLevel as ByteLevelPreTokenizer
from tokenizers.processors import (
    BertProcessing,
    ByteLevel,
    PostProcessor,
    RobertaProcessing,
    Sequence,
    TemplateProcessing,
)

from ..utils import data_dir, roberta_files


class TestBertProcessing:
    def test_instantiate(self):
        processor = BertProcessing(("[SEP]", 0), ("[CLS]", 1))
        assert processor is not None
        assert isinstance(processor, PostProcessor)
        assert isinstance(processor, BertProcessing)
        assert isinstance(
            pickle.loads(pickle.dumps(BertProcessing(("[SEP]", 0), ("[CLS]", 1)))),
            BertProcessing,
        )

    def test_processing(self):
        tokenizer = Tokenizer(BPE())
        tokenizer.add_special_tokens(["[SEP]", "[CLS]"])
        tokenizer.add_tokens(["my", "name", "is", "john", "pair"])
        tokenizer.post_processor = BertProcessing(("[SEP]", 0), ("[CLS]", 1))

        output = tokenizer.encode("my name", "pair")
        assert output.tokens == ["[CLS]", "my", "name", "[SEP]", "pair", "[SEP]"]
        assert output.ids == [1, 2, 3, 0, 6, 0]


class TestRobertaProcessing:
    def test_instantiate(self):
        processor = RobertaProcessing(("</s>", 1), ("<s>", 0))
        assert processor is not None
        assert isinstance(processor, PostProcessor)
        assert isinstance(processor, RobertaProcessing)
        assert isinstance(
            pickle.loads(pickle.dumps(RobertaProcessing(("</s>", 1), ("<s>", 0)))),
            RobertaProcessing,
        )

    def test_processing(self):
        tokenizer = Tokenizer(BPE())
        tokenizer.add_special_tokens(["<s>", "</s>"])
        tokenizer.add_tokens(["my", "name", "is", "john", "pair"])
        tokenizer.post_processor = RobertaProcessing(("</s>", 1), ("<s>", 0))

        output = tokenizer.encode("my name", "pair")
        assert output.tokens == ["<s>", "my", "name", "</s>", "</s>", "pair", "</s>"]
        assert output.ids == [0, 2, 3, 1, 1, 6, 1]


class TestByteLevelProcessing:
    def test_instantiate(self):
        assert ByteLevel() is not None
        assert ByteLevel(trim_offsets=True) is not None
        assert isinstance(ByteLevel(), PostProcessor)
        assert isinstance(ByteLevel(), ByteLevel)
        assert isinstance(pickle.loads(pickle.dumps(ByteLevel())), ByteLevel)

    def test_processing(self, roberta_files):
        # Deprecated in 0.9
        with pytest.deprecated_call():
            tokenizer = Tokenizer(BPE(roberta_files["vocab"], roberta_files["merges"]))
        tokenizer.pre_tokenizer = ByteLevelPreTokenizer(add_prefix_space=True)

        # Keeps original offsets
        output = tokenizer.encode("My name is John")
        assert output.tokens == ["ĠMy", "Ġname", "Ġis", "ĠJohn"]
        assert output.offsets == [(0, 2), (2, 7), (7, 10), (10, 15)]

        # Trims offsets when activated
        tokenizer.post_processor = ByteLevel(trim_offsets=True)
        output = tokenizer.encode("My name is John")
        assert output.tokens == ["ĠMy", "Ġname", "Ġis", "ĠJohn"]
        assert output.offsets == [(0, 2), (3, 7), (8, 10), (11, 15)]

    def test_manual_reload(self):
        byte_level = ByteLevel()
        state = json.loads(byte_level.__getstate__())
        reloaded = ByteLevel(**state)
        assert isinstance(reloaded, ByteLevel)


class TestTemplateProcessing:
    def get_bert(self):
        return TemplateProcessing(
            single=["[CLS]", "$0", "[SEP]"],
            pair=["[CLS]", "$A", "[SEP]", "$B:1", "[SEP]:1"],
            special_tokens=[("[CLS]", 1), ("[SEP]", 0)],
        )

    def get_roberta(self):
        return TemplateProcessing(
            single="<s> $0 </s>",
            pair="<s> $A </s> </s> $B </s>",
            special_tokens=[("<s>", 0), ("</s>", 1)],
        )

    def get_t5_squad(self):
        # >>> from transformers import AutoTokenizer
        # >>> tok = AutoTokenizer.from_pretrained("t5-small")
        # >>> tok.tokenize("question: ")
        # ['▁question', ':']
        # >>> tok.tokenize("context: ")
        # ['▁context', ':']
        # >>> tok.encode("context: ")
        # [2625, 10]
        # >>> tok.encode("question: ")
        # [822, 10]

        return TemplateProcessing(
            single=["$0"],
            pair=["Q", "$A", "C", "$B"],
            special_tokens=[
                {
                    "id": "Q",
                    "ids": [2625, 10],
                    "tokens": ["_question", ":"],
                },
                {
                    "id": "C",
                    "ids": [822, 10],
                    "tokens": ["_context", ":"],
                },
            ],
        )

    def test_instantiate(self):
        bert = self.get_bert()
        assert bert is not None
        assert isinstance(bert, PostProcessor)
        assert isinstance(bert, TemplateProcessing)
        assert isinstance(pickle.loads(pickle.dumps(bert)), TemplateProcessing)

        # It is absolutely legal to have tokens with spaces in the name:
        TemplateProcessing(
            single=["[ C L S ]", "Token with space"],
            special_tokens=[("[ C L S ]", 0), ("Token with space", 1)],
        )
        # Sequence identifiers must be well formed:
        with pytest.raises(Exception, match="Cannot build Piece"):
            TemplateProcessing(single="[CLS] $$ [SEP]")
        with pytest.raises(Exception, match="Cannot build Piece"):
            TemplateProcessing(single="[CLS] $A: [SEP]")
        # Special tokens must be provided when used in template:
        with pytest.raises(Exception, match="Missing SpecialToken\\(s\\) with id\\(s\\)"):
            TemplateProcessing(single=["[CLS]"])

    def test_bert_parity(self):
        tokenizer = Tokenizer(BPE())
        tokenizer.add_special_tokens(["[SEP]", "[CLS]"])
        tokenizer.add_tokens(["my", "name", "is", "john", "pair"])
        tokenizer.post_processor = BertProcessing(("[SEP]", 0), ("[CLS]", 1))

        original = tokenizer.encode("my name", "pair")

        tokenizer.post_processor = self.get_bert()
        template = tokenizer.encode("my name", "pair")
        assert original.ids == template.ids

    def test_roberta_parity(self):
        tokenizer = Tokenizer(BPE())
        tokenizer.add_special_tokens(["<s>", "</s>"])
        tokenizer.add_tokens(["my", "name", "is", "john", "pair"])
        tokenizer.post_processor = RobertaProcessing(("</s>", 1), ("<s>", 0))

        original = tokenizer.encode("my name is john", "pair")
        tokenizer.post_processor = self.get_roberta()
        template = tokenizer.encode("my name is john", "pair")
        assert original.ids == template.ids


class TestSequenceProcessing:
    def test_sequence_processing(self):
        assert Sequence([]) is not None
        assert Sequence([ByteLevel()]) is not None
        assert isinstance(Sequence([]), PostProcessor)
        assert isinstance(Sequence([]), Sequence)
        serialized = pickle.dumps(Sequence([]))
        assert isinstance(pickle.loads(serialized), Sequence)

    def test_post_process(self):
        byte_level = ByteLevel(trim_offsets=True)
        template = TemplateProcessing(
            single=["[CLS]", "$0", "[SEP]"],
            pair=["[CLS]:0", "$A", "[SEP]:0", "$B:1", "[SEP]:1"],
            special_tokens=[("[CLS]", 1), ("[SEP]", 0)],
        )

        tokenizer = Tokenizer(BPE())
        tokenizer.add_special_tokens(["[SEP]", "[CLS]"])
        tokenizer.add_tokens(["my", "name", "is", "Ġjohn", "pair"])
        tokenizer.post_processor = template

        # Before the sequence
        original = tokenizer.encode("my name is Ġjohn")
        assert original.ids == [1, 2, 3, 4, 5, 0]
        assert original.type_ids == [0, 0, 0, 0, 0, 0]
        assert original.offsets == [(0, 0), (0, 2), (3, 7), (8, 10), (11, 16), (0, 0)]
        pair = tokenizer.encode("my name is Ġjohn", "pair")
        # assert pair.ids == [1, 2, 3, 4, 5, 0, 6, 0]
        assert pair.type_ids == [0, 0, 0, 0, 0, 0, 1, 1]
        assert pair.offsets == [(0, 0), (0, 2), (3, 7), (8, 10), (11, 16), (0, 0), (0, 4), (0, 0)]

        processor = Sequence([byte_level, template])
        tokenizer.post_processor = processor

        original = tokenizer.encode("my name is Ġjohn")
        assert original.ids == [1, 2, 3, 4, 5, 0]
        assert original.type_ids == [0, 0, 0, 0, 0, 0]
        # Offsets ARE trimmed
        assert original.offsets == [(0, 0), (0, 2), (3, 7), (8, 10), (12, 16), (0, 0)]
        pair = tokenizer.encode("my name is Ġjohn", "pair")
        # assert pair.ids == [1, 2, 3, 4, 5, 0, 6, 0]
        assert pair.type_ids == [0, 0, 0, 0, 0, 0, 1, 1]
        assert pair.offsets == [(0, 0), (0, 2), (3, 7), (8, 10), (12, 16), (0, 0), (0, 4), (0, 0)]

    def test_items(self):
        processors = Sequence([RobertaProcessing(("</s>", 1), ("<s>", 0)), ByteLevel()])
        assert processors[0].__class__ == RobertaProcessing
        assert processors[1].__class__ == ByteLevel
        processors[0] = ByteLevel(add_prefix_space=False, trim_offsets=False, use_regex=False)
        print(processors[0])
        processors[0].add_prefix_space = True
        processors[0].trim_offsets = True
        processors[0].use_regex = True
        print(processors[0])
        assert processors[0].__class__ == ByteLevel
        assert processors[0].add_prefix_space
        assert processors[0].trim_offsets
        assert processors[0].use_regex
