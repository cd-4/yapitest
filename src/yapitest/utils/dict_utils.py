from typing import List, Dict, Any


def get_dict_depth(dic: Dict, keys: List[str]) -> Any:
    for k in keys:
        dic = dic[k]
    return dic
