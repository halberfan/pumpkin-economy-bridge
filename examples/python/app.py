"""Example Pumpkin plugin in Python that uses the economy bridge.

Adds /pymoney, which asks the bridge for the player's balance.
"""

import json

import wit_world
from componentize_py_types import Err, Ok
from wit_world.exports import Metadata as MetadataExport
from wit_world.exports.metadata import PluginMetadata
from wit_world.imports import command, ipc, logging, permission, text, uuid

BRIDGE = "economy_bridge"
PERMISSION = "py_economy_example:money"
MONEY_COMMAND_ID = 0


class EconomyError(Exception):
    def __init__(self, error):
        # e.g. "NoProvider" or {"InsufficientFunds": {"balance": 1200}}
        self.error = error
        super().__init__(str(error))


def bridge_call(request):
    """Sends an EconomyRequest through the bridge and returns the Ok value."""
    message = json.dumps({"Call": request}).encode()
    try:
        reply = ipc.send_ipc_message(BRIDGE, message)
    except Err:
        raise EconomyError({"Transport": f"plugin `{BRIDGE}` is not loaded"})
    if isinstance(reply, Err):
        raise EconomyError({"Transport": reply.value})

    result = json.loads(reply.value)
    if "Err" in result:
        raise EconomyError(result["Err"])
    return result["Ok"]


def balance(account):
    return bridge_call({"Balance": {"account": account}})["Amount"]


def format_amount(amount):
    return bridge_call({"Format": {"amount": amount}})["Text"]


def fail(message):
    return Err(command.CommandError_CommandFailed(text.TextComponent.text(message)))


class Metadata(MetadataExport):
    def get_metadata(self):
        return PluginMetadata(
            name="py_economy_example",
            version="0.1.0",
            authors=["Bjorn"],
            description="Python example that uses the economy bridge",
            dependencies=[BRIDGE],
            permissions=[],
        )


class WitWorld(wit_world.WitWorld):
    def init_plugin(self):
        pass

    def on_load(self, context):
        context.register_permission(
            permission.Permission(
                node=PERMISSION,
                description="Allows executing the /pymoney command",
                default=permission.PermissionDefault_Allow(),
                children=[],
            )
        )
        money = command.Command(["pymoney"], "Shows your balance (Python example)")
        money.execute_with_handler_id(MONEY_COMMAND_ID)
        context.register_command(money, PERMISSION)
        logging.log(logging.Level.INFO, "Python economy example loaded")

    def on_unload(self, context):
        pass

    def handle_command(self, command_id, sender, server, args):
        if command_id != MONEY_COMMAND_ID:
            return 0
        player = sender.as_player()
        if player is None:
            raise fail("This command can only be used by players.")
        account = uuid.to_string(player.get_id())
        try:
            amount = balance(account)
            sender.send_message(text.TextComponent.text(f"Balance: {format_amount(amount)}"))
        except EconomyError as e:
            if e.error == "NoProvider":
                raise fail("No economy plugin is installed.")
            raise fail(f"Economy error: {e.error}")
        return 1

    # Everything below is required by the Pumpkin plugin world but unused here

    def handle_event(self, event_id, server, event):
        return event

    def handle_command_suggestion(self, handler_id, sender, server, request):
        return command.CommandSuggestions(start=0, length=0, values=[])

    def handle_task(self, handler_id, server):
        pass

    def handle_ipc_message(self, sender, message):
        raise Err("This plugin cannot receive messages.")

    def handle_ai_goal_can_start(self, goal_id, server, entity):
        return False

    def handle_ai_goal_should_continue(self, goal_id, server, entity):
        return False

    def handle_ai_goal_start(self, goal_id, server, entity):
        pass

    def handle_ai_goal_tick(self, goal_id, server, entity):
        pass

    def handle_ai_goal_stop(self, goal_id, server, entity):
        pass

    def handle_generate_phase(self, generator_id, phase, chunk):
        pass
