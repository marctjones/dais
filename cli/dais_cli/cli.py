"""Main CLI entry point."""

import click
from rich.console import Console

from dais_cli import __version__
from dais_cli.commands import setup, post, followers, test, stats, db, actor, config_cmd, media, interact, notifications, moderation, block, follow, timeline, dm, deploy, doctor, search, auth
from dais_cli.tui.app import run_tui

console = Console()


@click.group()
@click.version_option(version=__version__, prog_name="dais")
@click.pass_context
def main(ctx):
    """dais - Manage your dais.social ActivityPub server.

    A command-line tool for creating posts, managing followers,
    and administering your personal ActivityPub instance.
    """
    ctx.ensure_object(dict)


@click.command()
def tui():
    """Launch interactive Terminal UI for managing dais."""
    run_tui()


# Register command groups
main.add_command(setup.setup)
main.add_command(config_cmd.config)
main.add_command(actor.actor)
main.add_command(auth.auth)
main.add_command(post.post)
main.add_command(media.media)
main.add_command(followers.followers)
main.add_command(follow.follow)
main.add_command(timeline.timeline)
main.add_command(interact.interact)
main.add_command(dm.dm)
main.add_command(notifications.notifications)
main.add_command(moderation.moderation)
main.add_command(block.block)
main.add_command(test.test)
main.add_command(stats.stats)
main.add_command(db.db)
main.add_command(deploy.deploy)
main.add_command(doctor.doctor)
main.add_command(search.search)
main.add_command(tui)


if __name__ == "__main__":
    main()
